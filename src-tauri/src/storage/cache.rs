use std::{path::Path, sync::Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    errors::AppError,
    translation::types::{Language, TranslationResult},
};

/// How many results are kept for answering a repeated request without the network.
const MAX_CACHE_ENTRIES: i64 = 1_000;
/// How many translations the history keeps. Favourites are kept on top of this.
const MAX_HISTORY_ENTRIES: i64 = 5_000;

/// The translation database. It holds two things that used to be one table:
///
/// - the cache, keyed by everything that shapes a result, which only exists to answer a
///   repeated request quickly and may be emptied at any time;
/// - the history, one entry per translated text and direction, which is what the user
///   browses, searches and marks as favourite.
pub struct TranslationCache {
    connection: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    pub source_text: String,
    pub translation: String,
    pub source_language: String,
    pub target_language: String,
    pub provider: String,
    pub model: String,
    pub created_at: i64,
    /// When the text was last translated or looked up again.
    pub used_at: i64,
    pub favorite: bool,
}

impl TranslationCache {
    pub fn backup_to(&self, path: &Path) -> Result<(), AppError> {
        if path.exists() {
            return Err(AppError::Database("拒绝覆盖已有历史备份".into()));
        }
        let connection = self.lock()?;
        connection.backup("main", path, None)?;
        let backup = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let integrity: String = backup.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(AppError::Database("历史备份完整性检查失败".into()));
        }
        Ok(())
    }

    pub fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| AppError::Database(error.to_string()))?;
        }
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    #[cfg(test)]
    fn in_memory() -> Result<Self, AppError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(mut connection: Connection) -> Result<Self, AppError> {
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS translation_cache (
               id INTEGER PRIMARY KEY,
               cache_key TEXT NOT NULL UNIQUE,
               source_text TEXT NOT NULL,
               source_language TEXT NOT NULL,
               target_language TEXT NOT NULL,
               translation TEXT NOT NULL,
               result_json TEXT NOT NULL,
               provider TEXT NOT NULL,
               model TEXT NOT NULL,
               created_at INTEGER NOT NULL DEFAULT (unixepoch()),
               last_used_at INTEGER NOT NULL DEFAULT (unixepoch()),
               hit_count INTEGER NOT NULL DEFAULT 0,
               favorite INTEGER NOT NULL DEFAULT 0
             );",
        )?;
        ensure_column(&connection, "favorite", "INTEGER NOT NULL DEFAULT 0")?;
        split_history_from_cache(&mut connection)?;
        connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_history_order
               ON translation_history(favorite DESC, last_used_at DESC, id DESC);",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, AppError> {
        self.connection
            .lock()
            .map_err(|_| AppError::Database("cache lock poisoned".into()))
    }

    /// A cached result. A hit also counts as using the text again, which moves it to the
    /// top of the history.
    pub fn get(&self, key: &str) -> Result<Option<TranslationResult>, AppError> {
        let connection = self.lock()?;
        let json: Option<String> = connection
            .query_row(
                "SELECT result_json FROM translation_cache WHERE cache_key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()?;
        let Some(json) = json else {
            return Ok(None);
        };
        let mut result: TranslationResult =
            serde_json::from_str(&json).map_err(|error| AppError::Database(error.to_string()))?;
        connection.execute(
            "UPDATE translation_cache SET last_used_at = unixepoch(), hit_count = hit_count + 1 WHERE cache_key = ?1",
            [key],
        )?;
        record_history(&connection, &result, &json)?;
        result.cached = true;
        Ok(Some(result))
    }

    pub fn history_result(&self, id: i64) -> Result<TranslationResult, AppError> {
        let connection = self.lock()?;
        let json: Option<String> = connection
            .query_row(
                "SELECT result_json FROM translation_history WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        let json = json.ok_or_else(|| AppError::Database("history entry was not found".into()))?;
        let mut result: TranslationResult =
            serde_json::from_str(&json).map_err(|error| AppError::Database(error.to_string()))?;
        result.cached = true;
        Ok(result)
    }

    /// Stores a finished translation: in the cache under `key`, and in the history.
    pub fn put(&self, key: &str, result: &TranslationResult) -> Result<(), AppError> {
        let mut connection = self.lock()?;
        let json =
            serde_json::to_string(result).map_err(|error| AppError::Database(error.to_string()))?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO translation_cache (
               cache_key, source_text, source_language, target_language,
               translation, result_json, provider, model
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(cache_key) DO UPDATE SET
               translation = excluded.translation,
               result_json = excluded.result_json,
               last_used_at = unixepoch()",
            params![
                key,
                result.source_text,
                result.detected_language.code(),
                result.target_language.code(),
                result.translation,
                json,
                result.provider,
                result.model,
            ],
        )?;
        prune_cache(&transaction, MAX_CACHE_ENTRIES)?;
        record_history(&transaction, result, &json)?;
        prune_history(&transaction, MAX_HISTORY_ENTRIES)?;
        transaction.commit()?;
        Ok(())
    }

    /// Empties the cache. The history, including favourites, is not touched.
    pub fn clear(&self) -> Result<usize, AppError> {
        Ok(self.lock()?.execute("DELETE FROM translation_cache", [])?)
    }

    /// Empties the history, optionally keeping favourites, and returns how many entries
    /// went. The cache goes with it: leaving copies of the same texts behind would make
    /// "clear history" mean less than it says.
    pub fn clear_history(&self, keep_favorites: bool) -> Result<usize, AppError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let removed = transaction.execute(
            "DELETE FROM translation_history WHERE ?1 = 0 OR favorite = 0",
            [i64::from(keep_favorites)],
        )?;
        transaction.execute("DELETE FROM translation_cache", [])?;
        transaction.commit()?;
        Ok(removed)
    }

    /// One page of the history: favourites first, then most recently used. `query` filters
    /// on the source text and the translation.
    pub fn history(
        &self,
        query: &str,
        favorite_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HistoryEntry>, AppError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, source_text, translation, source_language, target_language,
                    provider, model, created_at, last_used_at, favorite
             FROM translation_history
             WHERE (?1 = '' OR source_text LIKE '%' || ?1 || '%' ESCAPE '\\'
                              OR translation LIKE '%' || ?1 || '%' ESCAPE '\\')
               AND (?2 = 0 OR favorite = 1)
             ORDER BY favorite DESC, last_used_at DESC, id DESC
             LIMIT ?3 OFFSET ?4",
        )?;
        let rows = statement.query_map(
            params![
                escape_like(query.trim()),
                i64::from(favorite_only),
                limit.clamp(1, 200),
                offset.max(0)
            ],
            |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    source_text: row.get(1)?,
                    translation: row.get(2)?,
                    source_language: row.get(3)?,
                    target_language: row.get(4)?,
                    provider: row.get(5)?,
                    model: row.get(6)?,
                    created_at: row.get(7)?,
                    used_at: row.get(8)?,
                    favorite: row.get::<_, i64>(9)? != 0,
                })
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn set_favorite(&self, id: i64, favorite: bool) -> Result<(), AppError> {
        let changed = self.lock()?.execute(
            "UPDATE translation_history SET favorite = ?2 WHERE id = ?1",
            params![id, i64::from(favorite)],
        )?;
        if changed == 0 {
            return Err(AppError::Database("history entry was not found".into()));
        }
        Ok(())
    }

    /// Removes one history entry together with the cached results for the same text, so
    /// that nothing of it is left behind.
    pub fn delete_history_entry(&self, id: i64) -> Result<(), AppError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM translation_cache WHERE (source_text, source_language, target_language) IN (
               SELECT source_text, source_language, target_language FROM translation_history WHERE id = ?1
             )",
            [id],
        )?;
        let changed = transaction.execute("DELETE FROM translation_history WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(AppError::Database("history entry was not found".into()));
        }
        transaction.commit()?;
        Ok(())
    }

    /// How many results the cache holds.
    pub fn len(&self) -> Result<i64, AppError> {
        Ok(self
            .lock()?
            .query_row("SELECT COUNT(*) FROM translation_cache", [], |row| {
                row.get(0)
            })?)
    }

    /// How many entries the history holds.
    pub fn history_len(&self) -> Result<i64, AppError> {
        Ok(self
            .lock()?
            .query_row("SELECT COUNT(*) FROM translation_history", [], |row| {
                row.get(0)
            })?)
    }
}

/// Earlier versions used the cache table as the history as well, so emptying the cache
/// deleted favourites and the cache limit capped the history. The first time this runs on
/// such a database it creates the history table and copies every entry into it, keeping
/// ids, dates and favourites. Where one text had several cached results (from different
/// models), the favourite or else the most recently used one becomes its history entry.
fn split_history_from_cache(connection: &mut Connection) -> Result<(), AppError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'translation_history')",
        [],
        |row| row.get(0),
    )?;
    if exists {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE translation_history (
           id INTEGER PRIMARY KEY,
           source_text TEXT NOT NULL,
           source_language TEXT NOT NULL,
           target_language TEXT NOT NULL,
           translation TEXT NOT NULL,
           result_json TEXT NOT NULL,
           provider TEXT NOT NULL,
           model TEXT NOT NULL,
           created_at INTEGER NOT NULL DEFAULT (unixepoch()),
           last_used_at INTEGER NOT NULL DEFAULT (unixepoch()),
           favorite INTEGER NOT NULL DEFAULT 0,
           UNIQUE(source_text, source_language, target_language)
         );
         INSERT OR IGNORE INTO translation_history (
           id, source_text, source_language, target_language, translation, result_json,
           provider, model, created_at, last_used_at, favorite
         )
         SELECT id, source_text, source_language, target_language, translation, result_json,
                provider, model, created_at, last_used_at, favorite
         FROM translation_cache
         ORDER BY favorite DESC, last_used_at DESC, id DESC;",
    )?;
    transaction.commit()?;
    Ok(())
}

/// Adds a translation to the history, or refreshes the entry the same text already has.
fn record_history(
    connection: &Connection,
    result: &TranslationResult,
    json: &str,
) -> Result<(), AppError> {
    connection.execute(
        "INSERT INTO translation_history (
           source_text, source_language, target_language, translation, result_json, provider, model
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(source_text, source_language, target_language) DO UPDATE SET
           translation = excluded.translation,
           result_json = excluded.result_json,
           provider = excluded.provider,
           model = excluded.model,
           last_used_at = unixepoch()",
        params![
            result.source_text,
            result.detected_language.code(),
            result.target_language.code(),
            result.translation,
            json,
            result.provider,
            result.model,
        ],
    )?;
    Ok(())
}

/// Makes user text literal inside a `LIKE ... ESCAPE '\'` pattern.
fn escape_like(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(character, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn ensure_column(connection: &Connection, name: &str, definition: &str) -> Result<(), AppError> {
    let mut statement = connection.prepare("PRAGMA table_info(translation_cache)")?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for column in columns {
        if column? == name {
            return Ok(());
        }
    }
    connection.execute(
        &format!("ALTER TABLE translation_cache ADD COLUMN {name} {definition}"),
        [],
    )?;
    Ok(())
}

/// Keeps the `limit` most recently used cached results.
fn prune_cache(connection: &Connection, limit: i64) -> Result<(), AppError> {
    connection.execute(
        "DELETE FROM translation_cache
         WHERE id IN (
           SELECT id FROM translation_cache
           ORDER BY last_used_at DESC, id DESC
           LIMIT -1 OFFSET ?1
         )",
        [limit.max(0)],
    )?;
    Ok(())
}

/// Keeps every favourite and the `limit` most recently used other entries.
fn prune_history(connection: &Connection, limit: i64) -> Result<(), AppError> {
    connection.execute(
        "DELETE FROM translation_history
         WHERE id IN (
           SELECT id FROM translation_history
           WHERE favorite = 0
           ORDER BY last_used_at DESC, id DESC
           LIMIT -1 OFFSET ?1
         )",
        [limit.max(0)],
    )?;
    Ok(())
}

/// `endpoint` is the service address the result came from. Two services can share a
/// provider label and a model name; their results must not answer for each other.
/// `context` stands for whatever else shaped the wording: the glossary terms that applied
/// and the user's extra requirements.
pub fn cache_key(
    normalized_text: &str,
    source: Language,
    target: Language,
    provider: &str,
    model: &str,
    endpoint: &str,
    context: &str,
) -> String {
    let mut hasher = Sha256::new();
    for part in [
        normalized_text,
        source.code(),
        target.code(),
        provider,
        model,
        endpoint,
        context,
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result() -> TranslationResult {
        TranslationResult {
            source_text: "hello".into(),
            translation: "你好".into(),
            detected_language: Language::English,
            target_language: Language::Chinese,
            provider: "OpenAI Compatible".into(),
            model: "test".into(),
            cached: false,
            phonetic: None,
            part_of_speech: None,
            definitions: vec![],
            example: None,
        }
    }

    #[test]
    fn cache_key_is_stable_and_context_sensitive() {
        let key = |model, endpoint| {
            cache_key(
                "hello",
                Language::English,
                Language::Chinese,
                "p",
                model,
                endpoint,
                "",
            )
        };
        let other_terms = cache_key(
            "hello",
            Language::English,
            Language::Chinese,
            "p",
            "m",
            "https://a.example/v1",
            "formal tone",
        );
        assert_ne!(key("m", "https://a.example/v1"), other_terms);
        assert_eq!(
            key("m", "https://a.example/v1"),
            key("m", "https://a.example/v1")
        );
        assert_ne!(
            key("m", "https://a.example/v1"),
            key("m2", "https://a.example/v1")
        );
        assert_ne!(
            key("m", "https://a.example/v1"),
            key("m", "https://b.example/v1"),
            "the same model name at another service is another result"
        );
    }

    #[test]
    fn stores_and_reads_results() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.put("key", &result()).unwrap();
        let loaded = cache.get("key").unwrap().unwrap();
        assert_eq!(loaded.translation, "你好");
        assert!(loaded.cached);
    }

    #[test]
    fn clears_cached_results() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.put("key", &result()).unwrap();
        assert_eq!(cache.clear().unwrap(), 1);
        assert_eq!(cache.len().unwrap(), 0);
    }

    #[test]
    fn prunes_least_recent_entries_to_limit() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.put("first", &result()).unwrap();
        cache.put("second", &result()).unwrap();
        cache.put("third", &result()).unwrap();

        {
            let connection = cache.connection.lock().unwrap();
            prune_cache(&connection, 2).unwrap();
        }

        assert_eq!(cache.len().unwrap(), 2);
        assert!(cache.get("first").unwrap().is_none());
    }

    #[test]
    fn history_search_treats_wildcard_characters_literally() {
        let cache = TranslationCache::in_memory().unwrap();
        let mut percent = result();
        percent.source_text = "100% sure".into();
        cache.put("percent", &percent).unwrap();
        let mut snake = result();
        snake.source_text = "snake_case name".into();
        cache.put("snake", &snake).unwrap();
        cache.put("plain", &result()).unwrap();

        let sources = |query: &str| -> Vec<String> {
            let mut found: Vec<String> = cache
                .history(query, false, 50, 0)
                .unwrap()
                .into_iter()
                .map(|entry| entry.source_text)
                .collect();
            found.sort();
            found
        };
        assert_eq!(sources("%"), ["100% sure"]);
        assert_eq!(sources("_"), ["snake_case name"]);
        assert_eq!(sources("e_c"), ["snake_case name"]);
        assert_eq!(sources("\\"), Vec::<String>::new());
        assert_eq!(sources("").len(), 3);
    }

    #[test]
    fn searches_favorites_and_deletes_history() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.put("first", &result()).unwrap();
        let entries = cache.history("hello", false, 50, 0).unwrap();
        let cached = cache.history_result(entries[0].id).unwrap();
        assert!(cached.cached);
        assert_eq!(cached.source_text, entries[0].source_text);
        assert_eq!(entries.len(), 1);
        let id = entries[0].id;
        cache.set_favorite(id, true).unwrap();
        assert!(cache.history("", true, 50, 0).unwrap()[0].favorite);
        cache.delete_history_entry(id).unwrap();
        assert!(cache.history_result(id).is_err());
        assert!(cache.history("", false, 50, 0).unwrap().is_empty());
    }

    #[test]
    fn migrates_existing_cache_schema() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE translation_cache (
                   id INTEGER PRIMARY KEY,
                   cache_key TEXT NOT NULL UNIQUE,
                   source_text TEXT NOT NULL,
                   source_language TEXT NOT NULL,
                   target_language TEXT NOT NULL,
                   translation TEXT NOT NULL,
                   result_json TEXT NOT NULL,
                   provider TEXT NOT NULL,
                   model TEXT NOT NULL,
                   created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                   last_used_at INTEGER NOT NULL DEFAULT (unixepoch()),
                   hit_count INTEGER NOT NULL DEFAULT 0
                 );",
            )
            .unwrap();
        let mut historical = result();
        historical.source_text = "原文 Hello 🦀".into();
        historical.definitions = vec!["旧版本释义".into()];
        historical.example = Some("example 示例".into());
        historical.phonetic = Some("/həˈləʊ/".into());
        connection.execute("INSERT INTO translation_cache (id,cache_key,source_text,source_language,target_language,translation,result_json,provider,model,created_at,hit_count) VALUES (42,'old',?1,'en','zh',?2,?3,'阿里云百炼','old-model',123456,7)", params![historical.source_text, historical.translation, serde_json::to_string(&historical).unwrap()]).unwrap();
        let cache = TranslationCache::from_connection(connection).unwrap();
        let entry = &cache.history("", false, 10, 0).unwrap()[0];
        assert_eq!(entry.id, 42);
        assert_eq!(entry.created_at, 123456);
        assert_eq!(entry.source_text, historical.source_text);
        assert!(!entry.favorite);
        let loaded = cache.get("old").unwrap().unwrap();
        assert_eq!(loaded.definitions, historical.definitions);
        assert_eq!(loaded.phonetic, historical.phonetic);
        assert_eq!(loaded.example, historical.example);
        cache.set_favorite(42, true).unwrap();
        let connection = cache.connection.into_inner().unwrap();
        let cache = TranslationCache::from_connection(connection).unwrap();
        assert!(cache.history("", true, 10, 0).unwrap()[0].favorite);
        cache.put("migrated", &result()).unwrap();
        assert_eq!(cache.len().unwrap(), 2);
    }

    #[test]
    fn emptying_the_cache_leaves_the_history_and_its_favourites() {
        let cache = TranslationCache::in_memory().unwrap();
        let mut kept = result();
        kept.source_text = "keep me".into();
        cache.put("kept", &kept).unwrap();
        cache.put("other", &result()).unwrap();
        let id = cache
            .history("keep", false, 10, 0)
            .unwrap()
            .first()
            .unwrap()
            .id;
        cache.set_favorite(id, true).unwrap();

        assert_eq!(cache.clear().unwrap(), 2);
        assert_eq!(cache.len().unwrap(), 0);
        assert_eq!(cache.history_len().unwrap(), 2);
        assert!(cache.history("", true, 10, 0).unwrap()[0].favorite);
        assert!(
            cache.get("kept").unwrap().is_none(),
            "the cache really is empty"
        );
        assert_eq!(cache.history_result(id).unwrap().source_text, "keep me");

        // Clearing the history can spare favourites, and takes the cache with it.
        cache.put("again", &result()).unwrap();
        assert_eq!(cache.clear_history(true).unwrap(), 1);
        assert_eq!(cache.len().unwrap(), 0);
        let left = cache.history("", false, 10, 0).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].source_text, "keep me");
        assert_eq!(cache.clear_history(false).unwrap(), 1);
        assert_eq!(cache.history_len().unwrap(), 0);
    }

    #[test]
    fn one_text_is_one_history_entry_however_often_it_is_translated() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.put("model-a", &result()).unwrap();
        let mut newer = result();
        newer.model = "another".into();
        newer.translation = "您好".into();
        cache.put("model-b", &newer).unwrap();

        assert_eq!(cache.len().unwrap(), 2, "each model's result is cached");
        let entries = cache.history("", false, 10, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].translation, "您好");
        assert_eq!(entries[0].model, "another");

        // A cache hit after the entry was removed from the history puts it back.
        let id = entries[0].id;
        cache.put("kept", &result()).unwrap();
        cache.clear_history(false).unwrap();
        cache.put("model-a", &result()).unwrap();
        cache
            .lock()
            .unwrap()
            .execute("DELETE FROM translation_history", [])
            .unwrap();
        assert!(cache.get("model-a").unwrap().is_some());
        assert_eq!(cache.history_len().unwrap(), 1);
        let _ = id;
    }

    #[test]
    fn deleting_a_history_entry_removes_every_cached_result_for_its_text() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.put("model-a", &result()).unwrap();
        cache.put("model-b", &result()).unwrap();
        let mut other = result();
        other.source_text = "something else".into();
        cache.put("other", &other).unwrap();
        let id = cache.history("hello", false, 10, 0).unwrap()[0].id;

        cache.delete_history_entry(id).unwrap();
        assert!(cache.get("model-a").unwrap().is_none());
        assert!(cache.get("model-b").unwrap().is_none());
        assert!(cache.get("other").unwrap().is_some());
        assert!(cache.delete_history_entry(id).is_err());
    }

    #[test]
    fn history_outlives_the_cache_limit_and_keeps_favourites_beyond_its_own() {
        let cache = TranslationCache::in_memory().unwrap();
        for index in 0..6 {
            let mut entry = result();
            entry.source_text = format!("text {index}");
            cache.put(&format!("key {index}"), &entry).unwrap();
        }
        let favourite = cache.history("text 0", false, 10, 0).unwrap()[0].id;
        cache.set_favorite(favourite, true).unwrap();
        {
            let connection = cache.lock().unwrap();
            prune_cache(&connection, 2).unwrap();
            prune_history(&connection, 3).unwrap();
        }
        assert_eq!(cache.len().unwrap(), 2);
        let entries = cache.history("", false, 10, 0).unwrap();
        assert_eq!(entries.len(), 4, "three recent ones and the favourite");
        assert!(entries[0].favorite);
        assert_eq!(entries[0].source_text, "text 0");

        // Paging walks the same order without gaps or repeats.
        let first = cache.history("", false, 3, 0).unwrap();
        let second = cache.history("", false, 3, 3).unwrap();
        let mut ids: Vec<i64> = first.iter().chain(&second).map(|entry| entry.id).collect();
        assert_eq!(ids.len(), 4);
        ids.dedup();
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn splitting_an_old_database_keeps_favourites_when_one_text_had_several_results() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE translation_cache (
                   id INTEGER PRIMARY KEY,
                   cache_key TEXT NOT NULL UNIQUE,
                   source_text TEXT NOT NULL,
                   source_language TEXT NOT NULL,
                   target_language TEXT NOT NULL,
                   translation TEXT NOT NULL,
                   result_json TEXT NOT NULL,
                   provider TEXT NOT NULL,
                   model TEXT NOT NULL,
                   created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                   last_used_at INTEGER NOT NULL DEFAULT (unixepoch()),
                   hit_count INTEGER NOT NULL DEFAULT 0,
                   favorite INTEGER NOT NULL DEFAULT 0
                 );",
            )
            .unwrap();
        let json = serde_json::to_string(&result()).unwrap();
        for (id, key, translation, used, favorite) in [
            (1, "old-model", "旧译文", 100, 1),
            (2, "new-model", "新译文", 900, 0),
            (3, "unrelated", "别的", 500, 0),
        ] {
            let source = if id == 3 { "other" } else { "hello" };
            connection
                .execute(
                    "INSERT INTO translation_cache (id, cache_key, source_text, source_language, target_language, translation, result_json, provider, model, created_at, last_used_at, favorite)
                     VALUES (?1, ?2, ?3, 'en', 'zh', ?4, ?5, 'p', 'm', 50, ?6, ?7)",
                    params![id, key, source, translation, json, used, favorite],
                )
                .unwrap();
        }
        let cache = TranslationCache::from_connection(connection).unwrap();
        let entries = cache.history("", false, 10, 0).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            (
                entries[0].id,
                entries[0].favorite,
                entries[0].translation.as_str()
            ),
            (1, true, "旧译文"),
            "the favourite is the one that is kept"
        );
        assert_eq!(entries[0].created_at, 50);
        assert_eq!(entries[1].id, 3);
        assert_eq!(cache.len().unwrap(), 3, "the cache itself is untouched");
    }

    #[test]
    fn online_backup_contains_uncheckpointed_wal_history_and_favorites() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.sqlite3");
        let backup_path = directory.path().join("backup.sqlite3");
        let cache = TranslationCache::open(&path).unwrap();
        let mut historical = result();
        historical.source_text = "中文 English 🦀".into();
        cache.put("first", &historical).unwrap();
        let id = cache.history("", false, 10, 0).unwrap()[0].id;
        cache.set_favorite(id, true).unwrap();
        cache.put("second", &result()).unwrap();
        cache.backup_to(&backup_path).unwrap();
        let restored = TranslationCache::open(&backup_path).unwrap();
        assert_eq!(restored.len().unwrap(), 2);
        assert_eq!(
            restored.history("", true, 10, 0).unwrap()[0].source_text,
            historical.source_text
        );
        assert_eq!(restored.history("", true, 10, 0).unwrap()[0].id, id);
        assert!(cache.backup_to(&backup_path).is_err());
        cache.put("after-backup", &result()).unwrap();
        assert_eq!(cache.len().unwrap(), 3);
        assert_eq!(restored.len().unwrap(), 2);
        drop(cache);
        drop(restored);
        assert_eq!(TranslationCache::open(&path).unwrap().len().unwrap(), 3);
    }
}
