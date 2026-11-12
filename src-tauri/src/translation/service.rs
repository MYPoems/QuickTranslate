use std::sync::Arc;

use crate::{
    config::AppSettings,
    errors::AppError,
    providers::{OpenAiCompatibleProvider, ProviderConfig, Translator},
    storage::{HistoryEntry, TranslationCache},
};

use super::{
    language::detect_language,
    normalize::normalize_text,
    types::{Language, TranslationRequest, TranslationResult},
};

pub struct TranslationService {
    client: reqwest::Client,
    cache: Arc<TranslationCache>,
}

impl TranslationService {
    pub async fn backup_cache(&self, path: std::path::PathBuf) -> Result<(), AppError> {
        let cache = Arc::clone(&self.cache);
        tokio::task::spawn_blocking(move || cache.backup_to(&path))
            .await
            .map_err(|error| AppError::Internal(error.to_string()))?
    }
    pub fn new(client: reqwest::Client, cache: Arc<TranslationCache>) -> Self {
        Self { client, cache }
    }

    pub async fn translate(
        &self,
        text: String,
        settings: AppSettings,
        api_key: String,
    ) -> Result<TranslationResult, AppError> {
        self.translate_inner(text, settings, api_key, true).await
    }

    pub async fn translate_fresh(
        &self,
        text: String,
        settings: AppSettings,
        api_key: String,
    ) -> Result<TranslationResult, AppError> {
        self.translate_inner(text, settings, api_key, false).await
    }

    /// Translates and reports the text produced so far through `on_text` as it arrives.
    /// A cached result, a single-word lookup and a service that does not stream all finish
    /// without `on_text` being called.
    pub async fn translate_streaming(
        &self,
        text: String,
        settings: AppSettings,
        api_key: String,
        use_cache: bool,
        target: Option<Language>,
        on_text: impl FnMut(&str) + Send,
    ) -> Result<TranslationResult, AppError> {
        self.translate_with(text, settings, api_key, use_cache, target, Some(on_text))
            .await
    }

    async fn translate_inner(
        &self,
        text: String,
        settings: AppSettings,
        api_key: String,
        use_cache: bool,
    ) -> Result<TranslationResult, AppError> {
        self.translate_with(text, settings, api_key, use_cache, None, None::<fn(&str)>)
            .await
    }

    async fn translate_with(
        &self,
        text: String,
        settings: AppSettings,
        api_key: String,
        use_cache: bool,
        target: Option<Language>,
        on_text: Option<impl FnMut(&str) + Send>,
    ) -> Result<TranslationResult, AppError> {
        let text = normalize_text(&text)?;
        let source_language = detect_language(&text);
        let target_language = settings.translation.target_for(source_language, target);
        let glossary = settings.translation.glossary_for(&text);
        let key = crate::storage::cache_key(
            &text,
            source_language,
            target_language,
            &settings.provider,
            &settings.model,
            &settings.base_url,
            &settings.translation.cache_context(&glossary),
        );

        if use_cache {
            let cache = Arc::clone(&self.cache);
            let lookup_key = key.clone();
            if let Ok(Ok(Some(result))) =
                tokio::task::spawn_blocking(move || cache.get(&lookup_key)).await
            {
                return Ok(result);
            }
        }

        let provider = OpenAiCompatibleProvider::new(
            self.client.clone(),
            ProviderConfig {
                provider: settings.provider,
                base_url: settings.base_url,
                api_key,
                model: settings.model,
            },
        );
        let request = TranslationRequest {
            text,
            source_language,
            target_language,
            glossary,
            instructions: settings.translation.instructions,
        };
        // Only a finished translation is cached; a stream that fails half-way leaves nothing.
        let result = match on_text {
            Some(on_text) => provider.translate_stream(request, on_text).await?,
            None => provider.translate(request).await?,
        };

        let cache = Arc::clone(&self.cache);
        let cached_result = result.clone();
        let _ = tokio::task::spawn_blocking(move || cache.put(&key, &cached_result)).await;
        Ok(result)
    }

    pub async fn test_connection(
        &self,
        settings: AppSettings,
        api_key: String,
    ) -> Result<(), AppError> {
        if settings.base_url.trim().is_empty() || settings.model.trim().is_empty() {
            return Err(AppError::ProviderNotConfigured);
        }
        let provider = OpenAiCompatibleProvider::new(
            self.client.clone(),
            ProviderConfig {
                provider: settings.provider,
                base_url: settings.base_url,
                api_key,
                model: settings.model,
            },
        );
        provider
            .translate(TranslationRequest::new(
                "hello",
                Language::English,
                Language::Chinese,
            ))
            .await?;
        Ok(())
    }

    pub async fn clear_cache(&self) -> Result<usize, AppError> {
        let cache = Arc::clone(&self.cache);
        tokio::task::spawn_blocking(move || cache.clear())
            .await
            .map_err(|error| AppError::Internal(error.to_string()))?
    }

    pub async fn cache_size(&self) -> Result<i64, AppError> {
        let cache = Arc::clone(&self.cache);
        tokio::task::spawn_blocking(move || cache.len())
            .await
            .map_err(|error| AppError::Internal(error.to_string()))?
    }

    /// Empties the history, optionally keeping favourites; the cache goes with it.
    pub async fn clear_history(&self, keep_favorites: bool) -> Result<usize, AppError> {
        let cache = Arc::clone(&self.cache);
        tokio::task::spawn_blocking(move || cache.clear_history(keep_favorites))
            .await
            .map_err(|error| AppError::Internal(error.to_string()))?
    }

    pub async fn history_size(&self) -> Result<i64, AppError> {
        let cache = Arc::clone(&self.cache);
        tokio::task::spawn_blocking(move || cache.history_len())
            .await
            .map_err(|error| AppError::Internal(error.to_string()))?
    }

    pub async fn history(
        &self,
        query: String,
        favorite_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HistoryEntry>, AppError> {
        let cache = Arc::clone(&self.cache);
        tokio::task::spawn_blocking(move || cache.history(&query, favorite_only, limit, offset))
            .await
            .map_err(|error| AppError::Internal(error.to_string()))?
    }

    pub async fn history_result(&self, id: i64) -> Result<TranslationResult, AppError> {
        let cache = Arc::clone(&self.cache);
        tokio::task::spawn_blocking(move || cache.history_result(id))
            .await
            .map_err(|error| AppError::Internal(error.to_string()))?
    }

    pub async fn set_favorite(&self, id: i64, favorite: bool) -> Result<(), AppError> {
        let cache = Arc::clone(&self.cache);
        tokio::task::spawn_blocking(move || cache.set_favorite(id, favorite))
            .await
            .map_err(|error| AppError::Internal(error.to_string()))?
    }

    pub async fn delete_history_entry(&self, id: i64) -> Result<(), AppError> {
        let cache = Arc::clone(&self.cache);
        tokio::task::spawn_blocking(move || cache.delete_history_entry(id))
            .await
            .map_err(|error| AppError::Internal(error.to_string()))?
    }
}
