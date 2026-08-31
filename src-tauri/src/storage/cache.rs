use std::{path::Path, sync::Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::{
    errors::AppError,
    translation::types::{Language, TranslationResult},
};

pub struct TranslationCache {
    connection: Mutex<Connection>,
}

impl TranslationCache {
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

    fn from_connection(connection: Connection) -> Result<Self, AppError> {
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
               hit_count INTEGER NOT NULL DEFAULT 0
             );",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn get(&self, key: &str) -> Result<Option<TranslationResult>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::Database("cache lock poisoned".into()))?;
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
        connection.execute(
            "UPDATE translation_cache SET last_used_at = unixepoch(), hit_count = hit_count + 1 WHERE cache_key = ?1",
            [key],
        )?;
        let mut result: TranslationResult =
            serde_json::from_str(&json).map_err(|error| AppError::Database(error.to_string()))?;
        result.cached = true;
        Ok(Some(result))
    }

    pub fn put(&self, key: &str, result: &TranslationResult) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::Database("cache lock poisoned".into()))?;
        let json =
            serde_json::to_string(result).map_err(|error| AppError::Database(error.to_string()))?;
        connection.execute(
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
        Ok(())
    }
}

pub fn cache_key(
    normalized_text: &str,
    source: Language,
    target: Language,
    provider: &str,
    model: &str,
) -> String {
    let mut hasher = Sha256::new();
    for part in [
        normalized_text,
        source.code(),
        target.code(),
        provider,
        model,
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
        let first = cache_key("hello", Language::English, Language::Chinese, "p", "m");
        let second = cache_key("hello", Language::English, Language::Chinese, "p", "m");
        let changed = cache_key("hello", Language::English, Language::Chinese, "p", "m2");
        assert_eq!(first, second);
        assert_ne!(first, changed);
    }

    #[test]
    fn stores_and_reads_results() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.put("key", &result()).unwrap();
        let loaded = cache.get("key").unwrap().unwrap();
        assert_eq!(loaded.translation, "你好");
        assert!(loaded.cached);
    }
}
