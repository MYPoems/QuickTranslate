pub mod errors {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../src-tauri/src/errors.rs"
    ));
}

pub mod translation {
    pub mod types {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src-tauri/src/translation/types.rs"
        ));
    }
    pub mod language {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src-tauri/src/translation/language.rs"
        ));
    }
    pub mod normalize {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src-tauri/src/translation/normalize.rs"
        ));
    }
    pub mod prompt {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src-tauri/src/translation/prompt.rs"
        ));
    }
}

pub mod providers {
    use async_trait::async_trait;

    use crate::{
        errors::AppError,
        translation::types::{TranslationRequest, TranslationResult},
    };

    #[async_trait]
    pub trait Translator: Send + Sync {
        async fn translate(
            &self,
            request: TranslationRequest,
        ) -> Result<TranslationResult, AppError>;
    }

    pub mod openai_compatible {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src-tauri/src/providers/openai_compatible.rs"
        ));
    }
}

pub mod storage {
    pub mod cache {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src-tauri/src/storage/cache.rs"
        ));
    }
}
