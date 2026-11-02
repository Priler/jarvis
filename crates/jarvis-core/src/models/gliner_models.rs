use crate::APP_DIR;
use std::collections::HashMap;
use std::fs;

const GLINER_DIRS: &[&str] = &["gliner_small-v2.1", "gliner_multi-v2.1"];

#[derive(Debug, Clone)]
pub struct GlinerModelVariant {
    // type id stored in settings, e.g. "int8", "fp16", "full"
    pub value: String,
    // shown in dropdown, e.g. "int8 (174MB / 332MB)"
    pub display_name: String,
}

// scan both model dirs and return a deduplicated list of model types
pub fn scan_gliner_variants() -> Vec<GlinerModelVariant> {
    let base = APP_DIR.join("resources").join("models");

    // collect: type -> { dir_name -> size_mb }
    let mut types: HashMap<String, HashMap<String, u64>> = HashMap::new();

    for dir_name in GLINER_DIRS {
        let onnx_dir = base.join(dir_name).join("onnx");
        if !onnx_dir.exists() {
            continue;
        }

        let entries = match fs::read_dir(&onnx_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) if n.ends_with(".onnx") => n.to_string(),
                _ => continue,
            };

            let variant_type = file_name_to_type(&file_name);
            let size_mb = fs::metadata(&path).map(|m| m.len()).unwrap_or(0) / (1024 * 1024);

            types
                .entry(variant_type)
                .or_default()
                .insert(dir_name.to_string(), size_mb);
        }
    }

    let mut result: Vec<GlinerModelVariant> = types
        .into_iter()
        .map(|(variant, sizes)| {
            let size_str = build_size_string(&sizes);
            let label = if variant == "full" {
                "Full".to_string()
            } else {
                variant.clone()
            };
            GlinerModelVariant {
                display_name: format!("{} ({})", label, size_str),
                value: variant,
            }
        })
        .collect();

    // sort: full first, then alphabetically
    result.sort_by(|a, b| {
        let a_full = a.value == "full";
        let b_full = b.value == "full";
        b_full.cmp(&a_full).then_with(|| a.value.cmp(&b.value))
    });

    result
}

// "model.onnx" -> "full", "model_int8.onnx" -> "int8"
fn file_name_to_type(name: &str) -> String {
    let stem = name.strip_suffix(".onnx").unwrap_or(name);
    if stem == "model" {
        "full".to_string()
    } else if let Some(variant) = stem.strip_prefix("model_") {
        variant.to_string()
    } else {
        stem.to_string()
    }
}

// build size display: "174MB" if only one dir, "small: 174MB / multi: 332MB" if both
fn build_size_string(sizes: &HashMap<String, u64>) -> String {
    if sizes.len() == 1 {
        let (dir, mb) = sizes.iter().next().unwrap();
        let short = short_dir_name(dir);
        return format!("{}: {}MB", short, mb);
    }

    let mut parts: Vec<String> = Vec::new();
    for dir_name in GLINER_DIRS {
        if let Some(mb) = sizes.get(*dir_name) {
            parts.push(format!("{}: {}MB", short_dir_name(dir_name), mb));
        }
    }
    parts.join(" / ")
}

fn short_dir_name(dir: &str) -> &str {
    if dir.contains("small") {
        "small"
    } else if dir.contains("multi") {
        "multi"
    } else {
        dir
    }
}

// resolve variant type + language into actual file path
// returns (model_dir_path, onnx_file_name) or None
pub fn resolve_model(variant: &str, language: &str) -> Option<(std::path::PathBuf, String)> {
    let base = APP_DIR.join("resources").join("models");
    let file_name = type_to_file_name(variant);

    // pick dir based on language
    let preferred: &[&str] = match language {
        "en" => &["gliner_small-v2.1", "gliner_multi-v2.1"],
        _ => &["gliner_multi-v2.1", "gliner_small-v2.1"],
    };

    for dir_name in preferred {
        let path = base.join(dir_name).join("onnx").join(&file_name);
        if path.exists() {
            return Some((base.join(dir_name), file_name));
        }
    }

    None
}

// "full" -> "model.onnx", "int8" -> "model_int8.onnx"
fn type_to_file_name(variant: &str) -> String {
    if variant == "full" || variant.is_empty() {
        "model.onnx".to_string()
    } else {
        format!("model_{}.onnx", variant)
    }
}
