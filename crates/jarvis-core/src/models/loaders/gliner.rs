// GLiNER model for named entity recognition / slot extraction

use parking_lot::Mutex;
use regex::Regex;
use std::sync::Arc;
use tokenizers::Tokenizer;

use crate::models::registry::ModelRegistry;

const WORD_REGEX: &str = r"\w+(?:[-_]\w+)*|\S";

pub struct GlinerModel {
    pub session: Mutex<ort::session::Session>,
    pub tokenizer: Tokenizer,
    pub splitter: Regex,
}

unsafe impl Send for GlinerModel {}
unsafe impl Sync for GlinerModel {}

pub fn load(registry: &ModelRegistry, model_id: &str) -> Result<Arc<GlinerModel>, String> {
    registry.get_or_load::<GlinerModel>(model_id, |def| {
        let model_dir = &def.path;

        // GLiNER models keep onnx files in an "onnx" subfolder
        let onnx_dir = model_dir.join("onnx");
        let model_path = if onnx_dir.exists() {
            onnx_dir.join("model.onnx")
        } else {
            model_dir.join("model.onnx")
        };

        let tokenizer_path = model_dir.join("tokenizer.json");

        info!("Loading GLiNER model from: {}", model_dir.display());

        let session = ort::session::Session::builder()
            .map_err(|e| format!("Failed to create ORT session builder: {}", e))?
            .commit_from_file(&model_path)
            .map_err(|e| format!("Failed to load ONNX model: {}", e))?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

        let splitter =
            Regex::new(WORD_REGEX).map_err(|e| format!("Failed to compile word regex: {}", e))?;

        info!("GLiNER model loaded: {}", def.name);
        Ok(GlinerModel {
            session: Mutex::new(session),
            tokenizer,
            splitter,
        })
    })
}
