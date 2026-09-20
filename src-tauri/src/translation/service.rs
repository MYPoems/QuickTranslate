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
    types::{TranslationRequest, TranslationResult},
};

pub struct TranslationService {
    client: reqwest::Client,
    cache: Arc<TranslationCache>,
}

impl TranslationService {
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

    async fn translate_inner(
        &self,
        text: String,
        settings: AppSettings,
        api_key: String,
        use_cache: bool,
    ) -> Result<TranslationResult, AppError> {
        let text = normalize_text(&text)?;
        let source_language = detect_language(&text);
        let target_language = source_language.target();
        let key = crate::storage::cache_key(
            &text,
            source_language,
            target_language,
            &settings.provider,
            &settings.model,
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
                base_url: settings.base_url,
                api_key,
                model: settings.model,
            },
        );
        let result = provider
            .translate(TranslationRequest {
                text,
                source_language,
                target_language,
            })
            .await?;

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
                base_url: settings.base_url,
                api_key,
                model: settings.model,
            },
        );
        provider
            .translate(TranslationRequest {
                text: "hello".into(),
                source_language: super::types::Language::English,
                target_language: super::types::Language::Chinese,
            })
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

    pub async fn history(
        &self,
        query: String,
        favorite_only: bool,
        limit: i64,
    ) -> Result<Vec<HistoryEntry>, AppError> {
        let cache = Arc::clone(&self.cache);
        tokio::task::spawn_blocking(move || cache.history(&query, favorite_only, limit))
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
