// generic ORT model - session + optional tokenizer.
// for models like BERT (tiny, distil, mini) that can serve
// multiple tasks (intent, NER, text classification, etc.)

use parking_lot::Mutex;
use std::sync::Arc;
use tokenizers::Tokenizer;

use crate::models::registry::ModelRegistry;

pub struct OrtModel {
    pub session: Mutex<ort::session::Session>,
    pub tokenizer: Option<Tokenizer>,
}

unsafe impl Send for OrtModel {}
unsafe impl Sync for OrtModel {}

pub fn load(registry: &ModelRegistry, model_id: &str) -> Result<Arc<OrtModel>, String> {
    registry.get_or_load::<OrtModel>(model_id, |def| {
        let model_dir = &def.path;
        let onnx_path = model_dir.join("model.onnx");

        info!("Loading ORT model from: {}", model_dir.display());

        let session = ort::session::Session::builder()
            .map_err(|e| format!("ORT session builder error: {}", e))?
            .commit_from_file(&onnx_path)
            .map_err(|e| format!("Failed to load ONNX model '{}': {}", onnx_path.display(), e))?;

        let tokenizer_path = model_dir.join("tokenizer.json");
        let tokenizer = if tokenizer_path.exists() {
            Some(
                Tokenizer::from_file(&tokenizer_path)
                    .map_err(|e| format!("Failed to load tokenizer: {}", e))?,
            )
        } else {
            None
        };

        info!("ORT model loaded: {}", def.name);
        Ok(OrtModel {
            session: Mutex::new(session),
            tokenizer,
        })
    })
}
