// fastembed embedding model (all-MiniLM-L6-v2, paraphrase-multilingual, etc.)

use std::sync::Arc;
use parking_lot::Mutex;
use fastembed::{TextEmbedding, UserDefinedEmbeddingModel, TokenizerFiles, Pooling, QuantizationMode, OutputKey};

use crate::models::registry::ModelRegistry;

pub struct EmbeddingModel {
    pub embedding: Mutex<TextEmbedding>,
}

// fastembed uses ORT internally which is thread-safe
unsafe impl Send for EmbeddingModel {}
unsafe impl Sync for EmbeddingModel {}

pub fn load(registry: &ModelRegistry, model_id: &str) -> Result<Arc<EmbeddingModel>, String> {
    registry.get_or_load::<EmbeddingModel>(model_id, |def| {
        let model_dir = &def.path;

        // Pre-flight: verify all required files exist before attempting to read them.
        let required = [
            "model.onnx",
            "tokenizer.json",
            "config.json",
            "special_tokens_map.json",
            "tokenizer_config.json",
        ];
        let missing: Vec<&str> = required.iter()
            .copied()
            .filter(|f| !model_dir.join(f).exists())
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "Embedding model '{}' is missing files in {}: {}",
                model_id,
                model_dir.display(),
                missing.join(", ")
            ));
        }

        info!("Loading embedding model from: {}", model_dir.display());

        let user_model = UserDefinedEmbeddingModel {
            onnx_file: std::fs::read(model_dir.join("model.onnx"))
                .map_err(|e| format!("Failed to read model.onnx: {}", e))?,
            external_initializers: Vec::new(),
            tokenizer_files: TokenizerFiles {
                tokenizer_file: std::fs::read(model_dir.join("tokenizer.json"))
                    .map_err(|e| format!("Failed to read tokenizer.json: {}", e))?,
                config_file: std::fs::read(model_dir.join("config.json"))
                    .map_err(|e| format!("Failed to read config.json: {}", e))?,
                special_tokens_map_file: std::fs::read(model_dir.join("special_tokens_map.json"))
                    .map_err(|e| format!("Failed to read special_tokens_map.json: {}", e))?,
                tokenizer_config_file: std::fs::read(model_dir.join("tokenizer_config.json"))
                    .map_err(|e| format!("Failed to read tokenizer_config.json: {}", e))?,
            },
            pooling: Some(Pooling::Mean),
            quantization: QuantizationMode::None,
            output_key: Some(OutputKey::ByName("last_hidden_state")),
        };

        let model = TextEmbedding::try_new_from_user_defined(user_model, Default::default())
            .map_err(|e| format!("Failed to load embedding model: {}", e))?;

        info!("Embedding model loaded: {}", def.name);
        Ok(EmbeddingModel { embedding: Mutex::new(model) })
    })
}
