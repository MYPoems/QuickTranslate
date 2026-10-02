use crate::{
    app::AppState,
    errors::AppError,
    security::provider_api_key,
    vocabulary::{BookView, Entry, QuizResult, QuizView, Rules, VocabularyStore, WordCard},
};
use tauri::{AppHandle, Emitter, Manager};
fn now() -> Result<i64, AppError> {
    VocabularyStore::now()
}
#[tauri::command]
pub fn set_vocabulary_collection_open(
    open: bool,
    window: tauri::Window,
    app: AppHandle,
) -> Result<(), AppError> {
    if window.label() != "popup" {
        return Err(AppError::Vocabulary("仅悬浮窗可以改变收藏交互状态".into()));
    }
    app.state::<AppState>()
        .vocabulary_collection_open
        .store(open, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}
fn changed(app: &AppHandle) {
    let _ = app.emit("vocabulary-changed", ());
}
#[tauri::command]
pub fn open_vocabulary(app: AppHandle) {
    crate::window::show_vocabulary(&app);
}
#[tauri::command]
pub fn list_vocabulary(
    query: String,
    filter: String,
    offset: i64,
    app: AppHandle,
) -> Result<BookView, AppError> {
    app.state::<AppState>()
        .vocabulary
        .list(&query, &filter, offset, now()?)
}
#[tauri::command]
pub fn get_vocabulary_entry(id: i64, app: AppHandle) -> Result<Entry, AppError> {
    app.state::<AppState>().vocabulary.get(id)
}
#[tauri::command]
pub fn collect_vocabulary(
    word: String,
    sentence: String,
    translation: String,
    app: AppHandle,
) -> Result<Entry, AppError> {
    let e = app
        .state::<AppState>()
        .vocabulary
        .collect(&word, &sentence, &translation, now()?)?;
    changed(&app);
    Ok(e)
}
#[tauri::command]
pub async fn generate_vocabulary(
    id: i64,
    revision: i64,
    force: bool,
    quiz_only: bool,
    app: AppHandle,
) -> Result<Entry, AppError> {
    let state = app.state::<AppState>();
    let settings = state.settings.get()?;
    let entry = if quiz_only {
        let e = state.vocabulary.get(id)?;
        if e.revision != revision || e.card.is_none() {
            return Err(AppError::Vocabulary("请刷新词卡后重试".into()));
        }
        e
    } else {
        state.vocabulary.generation_begin(id, revision, force)?
    };
    if !quiz_only && entry.generation_state != "generating" {
        return Ok(entry);
    }
    changed(&app);
    let context = entry
        .sources
        .first()
        .map(|s| s.sentence.as_str())
        .unwrap_or("");
    let existing = if quiz_only { entry.card.as_ref() } else { None };
    let result = async {
        let _permit = state
            .vocabulary_generation
            .acquire()
            .await
            .map_err(|_| AppError::Vocabulary("词卡服务正在关闭".into()))?;
        let key = crate::vocabulary_model::cache_key(&settings, &entry.word, context, existing);
        if !force && !quiz_only {
            if let Some(card) = state.vocabulary.cached(&key)? {
                card.validate(&entry.word)?;
                return Ok(card);
            }
        }
        let api_key = provider_api_key(&settings, state.secrets.as_ref())?;
        let card = crate::vocabulary_model::generate(
            &state.http_client,
            &settings,
            &api_key,
            &entry.word,
            context,
            existing,
        )
        .await?;
        if !quiz_only {
            state.vocabulary.cache(&key, &card, now()?)?;
        }
        Ok::<_, AppError>(card)
    }
    .await;
    let e = if quiz_only {
        state
            .vocabulary
            .replace_quizzes(id, revision, result?.quizzes, now()?)?
    } else {
        state.vocabulary.generation_finish(
            id,
            entry.content_revision,
            result.map_err(|e| e.user_message()),
            &settings.model,
            &settings.provider,
            now()?,
        )?
    };
    changed(&app);
    Ok(e)
}
#[tauri::command]
pub fn review_vocabulary(
    id: i64,
    revision: i64,
    token: String,
    rating: String,
    app: AppHandle,
) -> Result<Entry, AppError> {
    let e = app
        .state::<AppState>()
        .vocabulary
        .review(id, revision, &token, &rating, now()?)?;
    changed(&app);
    Ok(e)
}
#[tauri::command]
pub fn save_vocabulary_card(
    id: i64,
    revision: i64,
    card: WordCard,
    app: AppHandle,
) -> Result<Entry, AppError> {
    let e = app
        .state::<AppState>()
        .vocabulary
        .save_card(id, revision, card, now()?)?;
    changed(&app);
    Ok(e)
}
#[tauri::command]
pub fn adopt_vocabulary_lemma(id: i64, revision: i64, app: AppHandle) -> Result<Entry, AppError> {
    let e = app
        .state::<AppState>()
        .vocabulary
        .adopt_lemma(id, revision, now()?)?;
    changed(&app);
    Ok(e)
}
#[tauri::command]
pub fn begin_vocabulary_quiz(id: i64, revision: i64, app: AppHandle) -> Result<QuizView, AppError> {
    let q = app
        .state::<AppState>()
        .vocabulary
        .begin_quiz(id, revision, now()?)?;
    changed(&app);
    Ok(q)
}
#[tauri::command]
pub fn submit_vocabulary_quiz(
    token: String,
    choice: usize,
    spelling: String,
    app: AppHandle,
) -> Result<QuizResult, AppError> {
    let r = app
        .state::<AppState>()
        .vocabulary
        .submit_quiz(&token, choice, &spelling, now()?)?;
    changed(&app);
    Ok(r)
}
#[tauri::command]
pub fn abandon_vocabulary_quiz(token: String, app: AppHandle) -> Result<(), AppError> {
    app.state::<AppState>().vocabulary.abandon_quiz(&token)?;
    changed(&app);
    Ok(())
}
#[tauri::command]
pub fn delete_vocabulary(
    id: i64,
    revision: i64,
    confirmed: bool,
    app: AppHandle,
) -> Result<(), AppError> {
    app.state::<AppState>()
        .vocabulary
        .delete(id, revision, confirmed)?;
    changed(&app);
    Ok(())
}
#[tauri::command]
pub fn set_vocabulary_rules(rules: Rules, app: AppHandle) -> Result<Rules, AppError> {
    let r = app
        .state::<AppState>()
        .vocabulary
        .set_rules(rules, now()?)?;
    changed(&app);
    Ok(r)
}
#[tauri::command]
pub fn export_vocabulary(app: AppHandle) -> Result<String, AppError> {
    app.state::<AppState>().vocabulary.export()
}
#[tauri::command]
pub fn import_vocabulary(json: String, confirmed: bool, app: AppHandle) -> Result<usize, AppError> {
    let count = app
        .state::<AppState>()
        .vocabulary
        .import(&json, confirmed, now()?)?;
    changed(&app);
    Ok(count)
}
