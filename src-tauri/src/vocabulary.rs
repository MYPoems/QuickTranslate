//! Local vocabulary is durable learning data, never an evictable translation cache.
use crate::errors::AppError;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub const MIN_INTERVAL: i64 = 4 * 3600;
const SCHEMA: i64 = 1;
fn fail(message: impl Into<String>) -> AppError {
    AppError::Vocabulary(message.into())
}
fn imported_token(id: i64, token: &str) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "import-{}",
        hex::encode(Sha256::digest(format!("{id}:{token}")))
    )
}
pub fn normalize_word(input: &str) -> Result<String, AppError> {
    let word = input.trim().replace('’', "'").to_ascii_lowercase();
    if word.is_empty()
        || word.len() > 48
        || !word
            .bytes()
            .all(|c| c.is_ascii_alphabetic() || c == b'\'' || c == b'-')
        || !word.as_bytes()[0].is_ascii_alphabetic()
        || !word.as_bytes()[word.len() - 1].is_ascii_alphabetic()
        || word.contains("--")
        || word.contains("''")
    {
        return Err(fail("请选取一个英文单词；可包含内部连字符或撇号"));
    }
    Ok(word)
}
fn bounded(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.contains('\0')
}
fn has_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphabetic() && c != '\'' && c != '-')
        .any(|token| token.eq_ignore_ascii_case(word))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Example {
    pub english: String,
    pub chinese: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuizSet {
    pub meaning_prompt: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub cloze: String,
    pub hint: String,
    pub answer: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordCard {
    pub word: String,
    pub lemma: String,
    pub ipa_uk: String,
    pub ipa_us: String,
    pub part_of_speech: String,
    pub definitions: Vec<String>,
    pub context_meaning: String,
    pub examples: Vec<Example>,
    pub collocations: Vec<String>,
    pub quizzes: Vec<QuizSet>,
}
impl WordCard {
    pub fn validate(&self, word: &str) -> Result<(), AppError> {
        if normalize_word(&self.word)? != word
            || normalize_word(&self.lemma).is_err()
            || self.ipa_uk.chars().count() > 100
            || self.ipa_us.chars().count() > 100
            || !bounded(&self.part_of_speech, 80)
            || !bounded(&self.context_meaning, 500)
            || self.definitions.is_empty()
            || self.definitions.len() > 6
            || self.definitions.iter().any(|v| !bounded(v, 300))
            || self.examples.is_empty()
            || self.examples.len() > 3
            || self
                .examples
                .iter()
                .any(|v| !bounded(&v.english, 600) || !bounded(&v.chinese, 600))
            || self.collocations.len() > 8
            || self.collocations.iter().any(|v| !bounded(v, 200))
            || self.quizzes.len() < 2
            || self.quizzes.len() > 4
        {
            return Err(fail("词卡内容不完整或超出限制，请编辑后重试或重新生成"));
        }
        let lemma = normalize_word(&self.lemma)?;
        for q in &self.quizzes {
            let unique: HashSet<_> = q.options.iter().map(|v| v.trim().to_lowercase()).collect();
            let answer = normalize_word(&q.answer)?;
            if !bounded(&q.meaning_prompt, 600)
                || q.options.len() != 4
                || unique.len() != 4
                || q.correct_index >= 4
                || q.options.iter().any(|v| !bounded(v, 200))
                || !bounded(&q.cloze, 600)
                || q.cloze.matches("____").count() != 1
                || !bounded(&q.hint, 300)
                || (answer != word && answer != lemma)
                || has_word(&q.cloze, word)
                || has_word(&q.cloze, &lemma)
                || has_word(&q.hint, word)
                || has_word(&q.hint, &lemma)
            {
                return Err(fail(
                    "测试题缺少唯一选项、正确答案或泄露拼写答案，请重新生成题目",
                ));
            }
        }
        if self.quizzes[0].cloze == self.quizzes[1].cloze {
            return Err(fail("至少需要两组不同测试题"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub sentence: String,
    pub translation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    pub word: String,
    pub card: Option<WordCard>,
    pub sources: Vec<Source>,
    pub status: String,
    pub reviews: u8,
    pub next_due_at: Option<i64>,
    pub studied_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub revision: i64,
    pub content_revision: i64,
    pub generation_state: String,
    pub generation_error: Option<String>,
    pub model: String,
    pub provider: String,
    pub generated_at: Option<i64>,
    pub user_edited: bool,
    pub active_quiz: Option<String>,
    pub quiz_attempts: u32,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rules {
    pub interval_hours: u16,
    pub daily_limit: u16,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            interval_hours: (MIN_INTERVAL / 3600) as u16,
            daily_limit: 20,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookView {
    pub entries: Vec<Entry>,
    pub total: i64,
    pub due: i64,
    pub tests: i64,
    pub mastered: i64,
    pub now: i64,
    pub rules: Rules,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuizView {
    pub token: String,
    pub entry_id: i64,
    pub meaning_prompt: String,
    pub options: Vec<String>,
    pub cloze: String,
    pub hint: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuizResult {
    pub passed: bool,
    pub meaning_correct: bool,
    pub spelling_correct: bool,
    pub entry: Entry,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedQuiz {
    view: QuizView,
    correct: usize,
    answer: String,
    revision: i64,
    created_at: i64,
    result: Option<QuizResult>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    token: String,
    entry_id: i64,
    action: String,
    at: i64,
    detail: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BookBackup {
    schema_version: i64,
    rules: Rules,
    entries: Vec<Entry>,
    events: Vec<Event>,
    quizzes: Vec<SavedQuiz>,
    aliases: Vec<(String, i64)>,
}
pub struct VocabularyStore {
    connection: Mutex<Connection>,
}

#[cfg(any(test, debug_assertions))]
pub fn fixture_card(word: &str) -> WordCard {
    let (meaning, sentence, cloze1, cloze2) = match word {
        "resilient" => (
            "有韧性的",
            "He remains resilient under pressure.",
            "He remains ____ under pressure.",
            "The ____ team recovered quickly.",
        ),
        "curious" => (
            "好奇的",
            "She is curious about the world.",
            "She is ____ about the world.",
            "A ____ child asked a question.",
        ),
        _ => (
            "建筑学",
            "She studies architecture at college.",
            "She studies ____ at college.",
            "The city's ____ is beautiful.",
        ),
    };
    let quiz = |cloze: &str| QuizSet {
        meaning_prompt: format!("What does {word} mean in: {sentence}"),
        options: vec![meaning.into(), "食物".into(), "天气".into(), "交通".into()],
        correct_index: 0,
        cloze: cloze.into(),
        hint: meaning.into(),
        answer: word.into(),
    };
    WordCard {
        word: word.into(),
        lemma: word.into(),
        ipa_uk: "".into(),
        ipa_us: "".into(),
        part_of_speech: if word == "architecture" { "n." } else { "adj." }.into(),
        definitions: vec![meaning.into()],
        context_meaning: meaning.into(),
        examples: vec![Example {
            english: sentence.into(),
            chinese: format!("示例：{meaning}"),
        }],
        collocations: vec![],
        quizzes: vec![quiz(cloze1), quiz(cloze2)],
    }
}

impl VocabularyStore {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| fail("无法创建生词本目录"))?;
        }
        Self::from_connection(Connection::open(path)?)
    }
    fn from_connection(mut c: Connection) -> Result<Self, AppError> {
        let version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA {
            return Err(fail("生词本来自更新版本，未覆盖；请升级应用"));
        }
        c.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;",
        )?;
        let tx = c.transaction()?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS words(id INTEGER PRIMARY KEY, word TEXT NOT NULL UNIQUE, state TEXT NOT NULL, due INTEGER, data TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS words_due ON words(state,due);
            CREATE TABLE IF NOT EXISTS aliases(word TEXT PRIMARY KEY, entry_id INTEGER NOT NULL REFERENCES words(id) ON DELETE CASCADE);
            CREATE TABLE IF NOT EXISTS events(token TEXT PRIMARY KEY,entry_id INTEGER NOT NULL REFERENCES words(id) ON DELETE CASCADE,action TEXT NOT NULL,at INTEGER NOT NULL,detail TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS quizzes(token TEXT PRIMARY KEY,entry_id INTEGER NOT NULL REFERENCES words(id) ON DELETE CASCADE,data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS generation_cache(key TEXT PRIMARY KEY,data TEXT NOT NULL,at INTEGER NOT NULL);
            PRAGMA user_version=1;")?;
        tx.commit()?;
        Ok(Self {
            connection: Mutex::new(c),
        })
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, AppError> {
        self.connection
            .lock()
            .map_err(|_| fail("生词本正在处理，请重试"))
    }
    pub fn now() -> Result<i64, AppError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|v| v.as_secs() as i64)
            .map_err(|_| fail("系统时间无效"))
    }
    fn clock(c: &Connection, now: i64) -> Result<(), AppError> {
        let last: i64 = c
            .query_row("SELECT value FROM metadata WHERE key='clock'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if now < last - 300 {
            return Err(fail(
                "检测到系统时间回拨，请恢复正确时间后再复习；进度未改变",
            ));
        }
        c.execute("INSERT INTO metadata VALUES('clock',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![now.max(last).to_string()])?;
        Ok(())
    }
    fn rules(c: &Connection) -> Result<Rules, AppError> {
        let value: Option<String> = c
            .query_row("SELECT value FROM metadata WHERE key='rules'", [], |r| {
                r.get(0)
            })
            .optional()?;
        value
            .map(|v| serde_json::from_str(&v).map_err(|_| fail("复习设置损坏")))
            .unwrap_or(Ok(Rules::default()))
    }
    fn read(c: &Connection, id: i64) -> Result<Entry, AppError> {
        let data: String = c
            .query_row("SELECT data FROM words WHERE id=?1", [id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| fail("词条已删除或不存在"))?;
        serde_json::from_str(&data).map_err(|_| fail("词条数据损坏"))
    }
    fn write(c: &Connection, e: &Entry) -> Result<(), AppError> {
        c.execute(
            "UPDATE words SET word=?1,state=?2,due=?3,data=?4 WHERE id=?5",
            params![
                e.word,
                e.status,
                e.next_due_at,
                serde_json::to_string(e).map_err(|_| fail("词条编码失败"))?,
                e.id
            ],
        )?;
        Ok(())
    }
    fn insert(c: &Connection, mut e: Entry) -> Result<Entry, AppError> {
        c.execute(
            "INSERT INTO words(word,state,due,data) VALUES(?1,?2,?3,'{}')",
            params![e.word, e.status, e.next_due_at],
        )?;
        e.id = c.last_insert_rowid();
        Self::write(c, &e)?;
        Ok(e)
    }
    fn revision(e: &Entry, revision: i64) -> Result<(), AppError> {
        if e.revision != revision {
            Err(fail("词条已在其他窗口改变，请刷新后重试"))
        } else {
            Ok(())
        }
    }
    fn event(
        tx: &Transaction,
        id: i64,
        token: &str,
        action: &str,
        now: i64,
        detail: &str,
    ) -> Result<bool, AppError> {
        if token.len() < 8
            || token.len() > 100
            || !token
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err(fail("操作标识无效"));
        }
        let prior: Option<(i64, String)> = tx
            .query_row(
                "SELECT entry_id,action FROM events WHERE token=?1",
                [token],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((entry, act)) = prior {
            if entry != id || act != action {
                return Err(fail("操作标识冲突"));
            }
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO events VALUES(?1,?2,?3,?4,?5)",
            params![token, id, action, now, detail],
        )?;
        Ok(true)
    }
    pub fn get(&self, id: i64) -> Result<Entry, AppError> {
        Self::read(&*self.lock()?, id)
    }
    pub fn collect(
        &self,
        word: &str,
        sentence: &str,
        translation: &str,
        now: i64,
    ) -> Result<Entry, AppError> {
        let word = normalize_word(word)?;
        if sentence.chars().count() > 1200
            || translation.chars().count() > 1200
            || sentence.contains('\0')
            || translation.contains('\0')
        {
            return Err(fail("来源句过长，最多 1200 字"));
        }
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        Self::clock(&tx, now)?;
        let id:Option<i64>=tx.query_row("SELECT id FROM words WHERE word=?1 UNION SELECT entry_id FROM aliases WHERE word=?1 LIMIT 1",[&word],|r|r.get(0)).optional()?;
        let source = Source {
            sentence: sentence.trim().into(),
            translation: translation.trim().into(),
        };
        let e = if let Some(id) = id {
            let mut e = Self::read(&tx, id)?;
            if !source.sentence.is_empty()
                && !e.sources.iter().any(|s| s.sentence == source.sentence)
            {
                if e.sources.len() >= 20 {
                    return Err(fail("该词已保存 20 条来源句，请先整理来源"));
                }
                e.sources.push(source);
                e.revision += 1;
                e.updated_at = now;
                Self::write(&tx, &e)?;
            }
            e
        } else {
            let count: i64 = tx.query_row("SELECT count(*) FROM words", [], |r| r.get(0))?;
            if count >= 5000 {
                return Err(fail("生词本最多 5000 词，请导出并整理后再添加"));
            }
            Self::insert(
                &tx,
                Entry {
                    id: 0,
                    word,
                    card: None,
                    sources: if source.sentence.is_empty() {
                        vec![]
                    } else {
                        vec![source]
                    },
                    status: "new".into(),
                    reviews: 0,
                    next_due_at: None,
                    studied_at: None,
                    created_at: now,
                    updated_at: now,
                    revision: 1,
                    content_revision: 1,
                    generation_state: "pending".into(),
                    generation_error: None,
                    model: String::new(),
                    provider: String::new(),
                    generated_at: None,
                    user_edited: false,
                    active_quiz: None,
                    quiz_attempts: 0,
                },
            )?
        };
        tx.commit()?;
        Ok(e)
    }
    pub fn list(
        &self,
        query: &str,
        filter: &str,
        offset: i64,
        now: i64,
    ) -> Result<BookView, AppError> {
        if query.chars().count() > 100
            || !matches!(filter, "learning" | "due" | "test" | "mastered" | "all")
        {
            return Err(fail("筛选条件无效"));
        }
        let c = self.lock()?;
        let rules = Self::rules(&c)?;
        let mut stmt=c.prepare("SELECT data FROM words WHERE instr(word,?1)>0 AND (?2='all' OR (?2='learning' AND state!='mastered') OR (?2='mastered' AND state='mastered') OR (?2='test' AND state='test') OR (?2='due' AND state='review' AND due<=?3)) ORDER BY CASE WHEN due IS NULL THEN 1 ELSE 0 END,due,id LIMIT 100 OFFSET ?4")?;
        let entries = stmt
            .query_map(
                params![
                    query.trim().to_ascii_lowercase(),
                    filter,
                    now,
                    offset.max(0)
                ],
                |r| r.get::<_, String>(0),
            )?
            .map(|r| serde_json::from_str(&r?).map_err(|_| fail("词条数据损坏")))
            .collect::<Result<Vec<_>, AppError>>()?;
        let (total,due,tests,mastered)=c.query_row("SELECT count(*),coalesce(sum(state='review' AND due<=?1),0),coalesce(sum(state='test'),0),coalesce(sum(state='mastered'),0) FROM words",[now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        Ok(BookView {
            entries,
            total,
            due,
            tests,
            mastered,
            now,
            rules,
        })
    }
    pub fn set_rules(&self, rules: Rules, now: i64) -> Result<Rules, AppError> {
        if !(4..=168).contains(&rules.interval_hours) || !(1..=100).contains(&rules.daily_limit) {
            return Err(fail("间隔须为 4–168 小时，每日上限为 1–100 词"));
        }
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        Self::clock(&tx, now)?;
        let old = Self::rules(&tx)?;
        if rules.interval_hours > old.interval_hours {
            let entries = {
                let mut s =
                    tx.prepare("SELECT data FROM words WHERE due>?1 AND state!='mastered'")?;
                let v = s
                    .query_map([now], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                v
            };
            for value in entries {
                let mut e: Entry =
                    serde_json::from_str(&value).map_err(|_| fail("词条数据损坏"))?;
                e.next_due_at = e
                    .next_due_at
                    .map(|v| v + i64::from(rules.interval_hours - old.interval_hours) * 3600);
                e.revision += 1;
                e.active_quiz = None;
                Self::write(&tx, &e)?;
            }
        }
        tx.execute("INSERT INTO metadata VALUES('rules',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(&rules).unwrap()])?;
        tx.commit()?;
        Ok(rules)
    }
    pub fn review(
        &self,
        id: i64,
        revision: i64,
        token: &str,
        rating: &str,
        now: i64,
    ) -> Result<Entry, AppError> {
        if !matches!(
            rating,
            "study" | "remember" | "fuzzy" | "forgot" | "restart"
        ) {
            return Err(fail("复习操作无效"));
        }
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        Self::clock(&tx, now)?;
        let mut e = Self::read(&tx, id)?;
        if !Self::event(&tx, id, token, rating, now, "")? {
            return Ok(e);
        }
        Self::revision(&e, revision)?;
        let rules = Self::rules(&tx)?;
        if e.generation_state == "generating" && rating != "restart" {
            return Err(fail("词卡正在更新，请等待完成再学习或复习"));
        }
        if rating == "restart" {
            e.reviews = 0;
            e.status = "new".into();
            e.next_due_at = None;
            e.studied_at = None;
            if e.generation_state == "generating" {
                e.content_revision += 1;
                e.generation_state = if e.card.is_some() { "ready" } else { "pending" }.into();
            }
        } else if rating == "study" {
            if e.status != "new" || e.card.is_none() {
                return Err(fail("请先补全词卡再完成首次学习"));
            }
            e.studied_at = Some(now);
            e.status = "review".into();
            e.next_due_at = Some(now + i64::from(rules.interval_hours) * 3600);
        } else {
            if e.status != "review" || e.card.is_none() || e.next_due_at.is_none_or(|v| v > now) {
                return Err(fail("尚未到复习时间，提前查看不计次数"));
            }
            let today = now / 86400;
            let done:i64=tx.query_row("SELECT count(*) FROM events WHERE action IN ('remember','fuzzy','forgot') AND at>=?1 AND at<?2",params![today*86400,(today+1)*86400],|r|r.get(0))?;
            // Current event is already in this transaction; failed eligibility rolls it back.
            if done > i64::from(rules.daily_limit) {
                return Err(fail("今日复习上限已达到（按 UTC 日统计），明日再继续"));
            }
            if rating == "remember" {
                e.reviews = (e.reviews + 1).min(3);
            } else if rating == "forgot" {
                e.reviews = e.reviews.saturating_sub(1);
            }
            e.status = if e.reviews == 3 { "test" } else { "review" }.into();
            e.next_due_at = Some(now + i64::from(rules.interval_hours) * 3600);
        }
        e.active_quiz = None;
        e.revision += 1;
        e.updated_at = now;
        Self::write(&tx, &e)?;
        tx.commit()?;
        Ok(e)
    }
    pub fn save_card(
        &self,
        id: i64,
        revision: i64,
        card: WordCard,
        now: i64,
    ) -> Result<Entry, AppError> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        Self::clock(&tx, now)?;
        let mut e = Self::read(&tx, id)?;
        Self::revision(&e, revision)?;
        card.validate(&e.word)?;
        e.card = Some(card);
        e.content_revision += 1;
        e.revision += 1;
        e.updated_at = now;
        e.user_edited = true;
        e.generation_state = "ready".into();
        e.generation_error = None;
        e.active_quiz = None;
        e.reviews = 0;
        e.status = "new".into();
        e.next_due_at = None;
        e.studied_at = None;
        Self::write(&tx, &e)?;
        tx.commit()?;
        Ok(e)
    }
    pub fn generation_begin(&self, id: i64, revision: i64, force: bool) -> Result<Entry, AppError> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        let mut e = Self::read(&tx, id)?;
        Self::revision(&e, revision)?;
        if e.card.is_some() && !force {
            return Ok(e);
        }
        if e.generation_state == "generating" {
            return Err(fail("该词正在生成，请稍候"));
        }
        if force && e.reviews > 0 {
            return Err(fail("重新生成会改变测试依据，请先点击重新学习，再生成词卡"));
        }
        e.generation_state = "generating".into();
        e.generation_error = None;
        e.content_revision += 1;
        e.revision += 1;
        Self::write(&tx, &e)?;
        tx.commit()?;
        Ok(e)
    }
    pub fn generation_finish(
        &self,
        id: i64,
        content_revision: i64,
        result: Result<WordCard, String>,
        model: &str,
        provider: &str,
        now: i64,
    ) -> Result<Entry, AppError> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        let mut e = Self::read(&tx, id)?;
        if e.content_revision != content_revision {
            return Err(fail("词卡已编辑，未覆盖新内容"));
        }
        match result {
            Ok(card) => {
                card.validate(&e.word)?;
                e.card = Some(card);
                e.generated_at = Some(now);
                e.model = model.into();
                e.provider = provider.into();
                e.user_edited = false;
                e.generation_state = "ready".into();
                e.reviews = 0;
                e.status = "new".into();
                e.next_due_at = None;
                e.studied_at = None;
            }
            Err(message) => {
                e.generation_state = if e.card.is_some() { "ready" } else { "pending" }.into();
                e.generation_error = Some(message.chars().take(500).collect());
            }
        }
        e.revision += 1;
        e.updated_at = now;
        e.active_quiz = None;
        Self::write(&tx, &e)?;
        tx.commit()?;
        Ok(e)
    }
    pub fn adopt_lemma(&self, id: i64, revision: i64, now: i64) -> Result<Entry, AppError> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        Self::clock(&tx, now)?;
        let mut e = Self::read(&tx, id)?;
        Self::revision(&e, revision)?;
        let lemma = normalize_word(&e.card.as_ref().ok_or_else(|| fail("请先生成词卡"))?.lemma)?;
        let target: Option<i64> = tx
            .query_row("SELECT id FROM words WHERE word=?1", [&lemma], |r| r.get(0))
            .optional()?;
        if let Some(other) = target.filter(|v| *v != id) {
            let mut dest = Self::read(&tx, other)?;
            for s in &e.sources {
                if !dest.sources.iter().any(|v| v.sentence == s.sentence) && dest.sources.len() < 20
                {
                    dest.sources.push(s.clone());
                }
            }
            tx.execute(
                "UPDATE aliases SET entry_id=?1 WHERE entry_id=?2",
                params![other, id],
            )?;
            tx.execute("INSERT INTO aliases VALUES(?1,?2) ON CONFLICT(word) DO UPDATE SET entry_id=excluded.entry_id",params![e.word,other])?;
            // Merging context does not certify a new sense; old target progress is preserved.
            dest.revision += 1;
            dest.updated_at = now;
            Self::write(&tx, &dest)?;
            tx.execute("DELETE FROM words WHERE id=?1", [id])?;
            tx.commit()?;
            return Ok(dest);
        }
        if lemma != e.word {
            tx.execute("INSERT INTO aliases VALUES(?1,?2) ON CONFLICT(word) DO UPDATE SET entry_id=excluded.entry_id",params![e.word,id])?;
            e.word = lemma;
            e.card = None;
            e.generation_state = "pending".into();
            e.reviews = 0;
            e.status = "new".into();
            e.studied_at = None;
            e.next_due_at = None;
            e.active_quiz = None;
            e.content_revision += 1;
            e.revision += 1;
            e.updated_at = now;
            Self::write(&tx, &e)?;
        }
        tx.commit()?;
        Ok(e)
    }
    pub fn replace_quizzes(
        &self,
        id: i64,
        revision: i64,
        quizzes: Vec<QuizSet>,
        now: i64,
    ) -> Result<Entry, AppError> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        Self::clock(&tx, now)?;
        let mut e = Self::read(&tx, id)?;
        Self::revision(&e, revision)?;
        let card = e.card.as_mut().ok_or_else(|| fail("词卡尚未补全"))?;
        card.quizzes = quizzes;
        card.validate(&e.word)?;
        e.active_quiz = None;
        e.content_revision += 1;
        e.revision += 1;
        e.updated_at = now;
        Self::write(&tx, &e)?;
        tx.commit()?;
        Ok(e)
    }
    pub fn begin_quiz(&self, id: i64, revision: i64, now: i64) -> Result<QuizView, AppError> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        Self::clock(&tx, now)?;
        let mut e = Self::read(&tx, id)?;
        Self::revision(&e, revision)?;
        if e.status != "test" || e.reviews != 3 || e.next_due_at.is_none_or(|v| v > now) {
            return Err(fail("需完成三次有效复习，并等待测试间隔后才能测试"));
        }
        let card = e.card.as_ref().ok_or_else(|| fail("词卡尚未补全"))?;
        card.validate(&e.word)?;
        let q = &card.quizzes[e.quiz_attempts as usize % card.quizzes.len()];
        let token = format!("quiz-{}-{}-{}", id, e.revision, now);
        let shift = (e.quiz_attempts as usize + e.revision as usize) % 4;
        let mut options = q.options.clone();
        options.rotate_left(shift);
        let view = QuizView {
            token: token.clone(),
            entry_id: id,
            meaning_prompt: q.meaning_prompt.clone(),
            options,
            cloze: q.cloze.clone(),
            hint: q.hint.clone(),
        };
        let saved = SavedQuiz {
            view: view.clone(),
            correct: (q.correct_index + 4 - shift) % 4,
            answer: normalize_word(&q.answer)?,
            revision: e.content_revision,
            created_at: now,
            result: None,
        };
        tx.execute(
            "INSERT INTO quizzes VALUES(?1,?2,?3)",
            params![token, id, serde_json::to_string(&saved).unwrap()],
        )?;
        e.quiz_attempts += 1;
        e.active_quiz = Some(token);
        e.revision += 1;
        Self::write(&tx, &e)?;
        tx.commit()?;
        Ok(view)
    }
    pub fn submit_quiz(
        &self,
        token: &str,
        choice: usize,
        spelling: &str,
        now: i64,
    ) -> Result<QuizResult, AppError> {
        if token.len() > 100 || spelling.len() > 100 || choice >= 4 {
            return Err(fail("测试答案无效"));
        }
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        Self::clock(&tx, now)?;
        let value: String = tx
            .query_row("SELECT data FROM quizzes WHERE token=?1", [token], |r| {
                r.get(0)
            })
            .optional()?
            .ok_or_else(|| fail("测试已失效，请重新开始"))?;
        let mut q: SavedQuiz = serde_json::from_str(&value).map_err(|_| fail("测试数据损坏"))?;
        if let Some(result) = q.result {
            return Ok(result);
        }
        let mut e = Self::read(&tx, q.view.entry_id)?;
        if e.active_quiz.as_deref() != Some(token)
            || e.content_revision != q.revision
            || e.status != "test"
            || now < q.created_at
            || now - q.created_at > 86400
        {
            return Err(fail("测试已过期或词条改变，请刷新后重试"));
        }
        let meaning_correct = choice == q.correct;
        let spelling_correct = normalize_word(spelling).is_ok_and(|v| v == q.answer);
        let passed = meaning_correct && spelling_correct;
        e.status = if passed { "mastered" } else { "review" }.into();
        e.reviews = if passed { 3 } else { 2 };
        e.next_due_at = if passed {
            None
        } else {
            Some(now + i64::from(Self::rules(&tx)?.interval_hours) * 3600)
        };
        e.revision += 1;
        e.active_quiz = None;
        e.updated_at = now;
        let result = QuizResult {
            passed,
            meaning_correct,
            spelling_correct,
            entry: e.clone(),
        };
        q.result = Some(result.clone());
        Self::write(&tx, &e)?;
        tx.execute(
            "UPDATE quizzes SET data=?1 WHERE token=?2",
            params![serde_json::to_string(&q).unwrap(), token],
        )?;
        Self::event(
            &tx,
            e.id,
            token,
            "quiz",
            now,
            if passed { "passed" } else { "failed" },
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub fn abandon_quiz(&self, token: &str) -> Result<(), AppError> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        let id: Option<i64> = tx
            .query_row(
                "SELECT entry_id FROM quizzes WHERE token=?1",
                [token],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = id {
            let mut e = Self::read(&tx, id)?;
            if e.active_quiz.as_deref() == Some(token) {
                e.active_quiz = None;
                e.revision += 1;
                Self::write(&tx, &e)?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn delete(&self, id: i64, revision: i64, confirmed: bool) -> Result<(), AppError> {
        if !confirmed {
            return Err(fail("删除需要确认"));
        }
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        Self::revision(&Self::read(&tx, id)?, revision)?;
        tx.execute("DELETE FROM words WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }
    pub fn cached(&self, key: &str) -> Result<Option<WordCard>, AppError> {
        let c = self.lock()?;
        let data: Option<String> = c
            .query_row(
                "SELECT data FROM generation_cache WHERE key=?1",
                [key],
                |r| r.get(0),
            )
            .optional()?;
        data.map(|v| serde_json::from_str(&v).map_err(|_| fail("词卡缓存损坏")))
            .transpose()
    }
    pub fn cache(&self, key: &str, card: &WordCard, now: i64) -> Result<(), AppError> {
        let c = self.lock()?;
        c.execute("INSERT INTO generation_cache VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET data=excluded.data,at=excluded.at",params![key,serde_json::to_string(card).unwrap(),now])?;
        c.execute("DELETE FROM generation_cache WHERE key NOT IN (SELECT key FROM generation_cache ORDER BY at DESC LIMIT 1000)",[])?;
        Ok(())
    }
    pub fn backup_to(&self, path: &Path) -> Result<(), AppError> {
        if path.exists() {
            return Err(fail("拒绝覆盖已有生词本备份"));
        }
        let c = self.lock()?;
        c.backup("main", path, None)?;
        let copied = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let check: String = copied.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(fail("生词本备份完整性检查失败"));
        }
        Ok(())
    }
    pub fn export(&self) -> Result<String, AppError> {
        let c = self.lock()?;
        let entries = {
            let mut s = c.prepare("SELECT data FROM words ORDER BY id")?;
            let v = s
                .query_map([], |r| r.get::<_, String>(0))?
                .map(|v| serde_json::from_str(&v?).map_err(|_| fail("词条数据损坏")))
                .collect::<Result<Vec<Entry>, AppError>>()?;
            v
        };
        let events = {
            let mut s =
                c.prepare("SELECT token,entry_id,action,at,detail FROM events ORDER BY at,token")?;
            let v = s
                .query_map([], |r| {
                    Ok(Event {
                        token: r.get(0)?,
                        entry_id: r.get(1)?,
                        action: r.get(2)?,
                        at: r.get(3)?,
                        detail: r.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            v
        };
        let quizzes = {
            let mut s = c.prepare("SELECT data FROM quizzes ORDER BY token")?;
            let v = s
                .query_map([], |r| r.get::<_, String>(0))?
                .map(|v| serde_json::from_str(&v?).map_err(|_| fail("测试数据损坏")))
                .collect::<Result<Vec<SavedQuiz>, AppError>>()?;
            v
        };
        serde_json::to_string_pretty(&BookBackup {
            schema_version: SCHEMA,
            rules: Self::rules(&c)?,
            entries,
            events,
            quizzes,
            aliases: {
                let mut s = c.prepare("SELECT word,entry_id FROM aliases ORDER BY word")?;
                let rows = s
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            },
        })
        .map_err(|_| fail("生词本导出失败"))
    }
    pub fn import(&self, json: &str, confirmed: bool, now: i64) -> Result<usize, AppError> {
        if !confirmed || json.len() > 20 * 1024 * 1024 {
            return Err(fail("请确认导入，备份最多 20 MiB"));
        }
        let backup: BookBackup =
            serde_json::from_str(json).map_err(|_| fail("生词本备份格式无效"))?;
        if backup.schema_version != SCHEMA
            || backup.entries.len() > 5000
            || backup.events.len() > 100000
            || backup.quizzes.len() > 20000
            || backup.aliases.len() > 10000
            || !(4..=168).contains(&backup.rules.interval_hours)
            || !(1..=100).contains(&backup.rules.daily_limit)
        {
            return Err(fail("备份版本或规模不支持，未修改生词本"));
        }
        let mut names = HashSet::new();
        let mut ids = HashSet::new();
        for e in &backup.entries {
            if normalize_word(&e.word)? != e.word
                || !names.insert(e.word.clone())
                || !ids.insert(e.id)
                || e.id <= 0
                || e.reviews > 3
                || !matches!(e.status.as_str(), "new" | "review" | "test" | "mastered")
                || e.sources.len() > 20
                || e.sources.iter().any(|s| {
                    s.sentence.chars().count() > 1200 || s.translation.chars().count() > 1200
                })
                || e.revision < 1
                || e.content_revision < 1
                || !matches!(
                    e.generation_state.as_str(),
                    "pending" | "generating" | "ready"
                )
                || e.generation_error
                    .as_ref()
                    .is_some_and(|v| v.chars().count() > 500)
                || (e.status != "new" && (e.card.is_none() || e.studied_at.is_none()))
                || (e.status == "new" && (e.reviews != 0 || e.next_due_at.is_some()))
                || (e.status == "review" && e.reviews >= 3)
                || e.model.len() > 200
                || e.provider.len() > 200
                || e.next_due_at
                    .is_some_and(|v| v < 0 || v > now + 365 * 86400)
                || (e.status == "test" || e.status == "mastered") && e.reviews != 3
                || (e.status == "review" || e.status == "test") && e.next_due_at.is_none()
            {
                return Err(fail("备份包含无效词条或学习进度"));
            }
            if let Some(card) = &e.card {
                card.validate(&e.word)?;
            }
        }
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        Self::clock(&tx, now)?;
        let count: i64 = tx.query_row("SELECT count(*) FROM words", [], |r| r.get(0))?;
        let mut mapping = std::collections::HashMap::new();
        let mut imported = 0;
        for mut e in backup.entries {
            let existing: bool = tx.query_row(
                "SELECT exists(SELECT 1 FROM words WHERE word=?1 UNION SELECT 1 FROM aliases WHERE word=?1)",
                [&e.word],
                |r| r.get(0),
            )?;
            if existing {
                continue;
            }
            if count + imported as i64 >= 5000 {
                return Err(fail("导入后超过生词本容量，未修改"));
            }
            let old = e.id;
            e.active_quiz = None;
            if e.generation_state == "generating" {
                e.generation_state = if e.card.is_some() { "ready" } else { "pending" }.into();
            }
            let inserted = Self::insert(&tx, e)?;
            mapping.insert(old, inserted.id);
            imported += 1;
        }
        for event in backup.events {
            if let Some(id) = mapping.get(&event.entry_id) {
                if event.token.len() > 100 || event.action.len() > 30 || event.detail.len() > 1000 {
                    return Err(fail("备份日志无效"));
                }
                tx.execute(
                    "INSERT INTO events VALUES(?1,?2,?3,?4,?5)",
                    params![
                        imported_token(*id, &event.token),
                        id,
                        event.action,
                        event.at,
                        event.detail
                    ],
                )?;
            }
        }
        for (alias, old_id) in backup.aliases {
            if normalize_word(&alias)? != alias {
                return Err(fail("基本词形别名无效"));
            }
            if let Some(id) = mapping.get(&old_id) {
                tx.execute(
                    "INSERT INTO aliases VALUES(?1,?2) ON CONFLICT(word) DO NOTHING",
                    params![alias, id],
                )?;
            }
        }
        for mut q in backup.quizzes {
            if let Some(id) = mapping.get(&q.view.entry_id) {
                if q.result.is_none() {
                    continue;
                }
                q.view.entry_id = *id;
                q.view.token = imported_token(*id, &q.view.token);
                if let Some(result) = q.result.as_mut() {
                    result.entry.id = *id;
                }
                tx.execute(
                    "INSERT INTO quizzes VALUES(?1,?2,?3)",
                    params![q.view.token, id, serde_json::to_string(&q).unwrap()],
                )?;
            }
        }
        // Restoring rules may not shorten existing due times; keep current rules on merge.
        if count == 0 {
            tx.execute("INSERT INTO metadata VALUES('rules',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(&backup.rules).unwrap()])?;
        }
        tx.commit()?;
        Ok(imported)
    }
    pub fn recover_interrupted(&self) -> Result<(), AppError> {
        let c = self.lock()?;
        let values = {
            let mut s = c.prepare("SELECT data FROM words")?;
            let v = s
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            v
        };
        for value in values {
            let mut e: Entry = serde_json::from_str(&value).map_err(|_| fail("词条损坏"))?;
            if e.generation_state == "generating" {
                e.generation_state = if e.card.is_some() { "ready" } else { "pending" }.into();
                e.generation_error = Some("上次生成被中断，可手动重试".into());
                e.revision += 1;
                Self::write(&c, &e)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "vocabulary_tests.rs"]
mod tests;
