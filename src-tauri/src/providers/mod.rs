mod openai_compatible;

use async_trait::async_trait;

use crate::{
    errors::AppError,
    translation::types::{TranslationRequest, TranslationResult},
};

pub use openai_compatible::{OpenAiCompatibleProvider, ProviderConfig};

#[async_trait]
pub trait Translator: Send + Sync {
    async fn translate(&self, request: TranslationRequest) -> Result<TranslationResult, AppError>;
}
