use std::{path::Path, sync::Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    errors::AppError,
    translation::types::{Language, TranslationResult},
};

const MAX_CACHE_ENTRIES: i64 = 1_000;

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
    pub favorite: bool,
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
               hit_count INTEGER NOT NULL DEFAULT 0,
               favorite INTEGER NOT NULL DEFAULT 0
             );",
        )?;
        ensure_column(&connection, "favorite", "INTEGER NOT NULL DEFAULT 0")?;
        connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_translation_history
               ON translation_cache(favorite DESC, last_used_at DESC, id DESC);",
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
        prune_to_limit(&connection, MAX_CACHE_ENTRIES)?;
        Ok(())
    }

    pub fn clear(&self) -> Result<usize, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::Database("cache lock poisoned".into()))?;
        Ok(connection.execute("DELETE FROM translation_cache", [])?)
    }

    pub fn history(
        &self,
        query: &str,
        favorite_only: bool,
        limit: i64,
    ) -> Result<Vec<HistoryEntry>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::Database("cache lock poisoned".into()))?;
        let mut statement = connection.prepare(
            "SELECT id, source_text, translation, source_language, target_language,
                    provider, model, created_at, favorite
             FROM translation_cache
             WHERE (?1 = '' OR source_text LIKE '%' || ?1 || '%' OR translation LIKE '%' || ?1 || '%')
               AND (?2 = 0 OR favorite = 1)
             ORDER BY favorite DESC, last_used_at DESC, id DESC
             LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![query.trim(), i64::from(favorite_only), limit.clamp(1, 200)],
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
                    favorite: row.get::<_, i64>(8)? != 0,
                })
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn set_favorite(&self, id: i64, favorite: bool) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::Database("cache lock poisoned".into()))?;
        let changed = connection.execute(
            "UPDATE translation_cache SET favorite = ?2 WHERE id = ?1",
            params![id, i64::from(favorite)],
        )?;
        if changed == 0 {
            return Err(AppError::Database("history entry was not found".into()));
        }
        Ok(())
    }

    pub fn delete_history_entry(&self, id: i64) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::Database("cache lock poisoned".into()))?;
        let changed = connection.execute("DELETE FROM translation_cache WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(AppError::Database("history entry was not found".into()));
        }
        Ok(())
    }

    pub fn len(&self) -> Result<i64, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::Database("cache lock poisoned".into()))?;
        Ok(
            connection.query_row("SELECT COUNT(*) FROM translation_cache", [], |row| {
                row.get(0)
            })?,
        )
    }
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

fn prune_to_limit(connection: &Connection, limit: i64) -> Result<(), AppError> {
    connection.execute(
        "DELETE FROM translation_cache
         WHERE id IN (
           SELECT id FROM translation_cache
           ORDER BY favorite DESC, last_used_at DESC, id DESC
           LIMIT -1 OFFSET ?1
         )",
        [limit.max(0)],
    )?;
    Ok(())
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
            prune_to_limit(&connection, 2).unwrap();
        }

        assert_eq!(cache.len().unwrap(), 2);
        assert!(cache.get("first").unwrap().is_none());
    }

    #[test]
    fn searches_favorites_and_deletes_history() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.put("first", &result()).unwrap();
        let entries = cache.history("hello", false, 50).unwrap();
        assert_eq!(entries.len(), 1);
        let id = entries[0].id;
        cache.set_favorite(id, true).unwrap();
        assert!(cache.history("", true, 50).unwrap()[0].favorite);
        cache.delete_history_entry(id).unwrap();
        assert!(cache.history("", false, 50).unwrap().is_empty());
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
        let cache = TranslationCache::from_connection(connection).unwrap();
        cache.put("migrated", &result()).unwrap();
        assert!(!cache.history("", false, 10).unwrap()[0].favorite);
    }
}
