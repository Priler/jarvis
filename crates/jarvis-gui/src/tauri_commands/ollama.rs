use serde::Deserialize;

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

fn is_localhost_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.starts_with("http://localhost")
        || lower.starts_with("http://127.0.0.1")
        || lower.starts_with("https://localhost")
        || lower.starts_with("https://127.0.0.1")
}

#[tauri::command]
pub async fn list_ollama_models(url: String) -> Result<Vec<String>, String> {
    if !is_localhost_url(&url) {
        return Err("Ollama URL must point to localhost".to_string());
    }
    let endpoint = format!("{}/api/tags", url.trim_end_matches('/'));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&endpoint)
        .send()
        .await
        .map_err(|e| format!("Нет соединения с Ollama: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Ollama вернула статус {}", resp.status()));
    }

    let tags: OllamaTagsResponse = resp
        .json()
        .await
        .map_err(|e| format!("Ошибка разбора ответа: {}", e))?;

    Ok(tags.models.into_iter().map(|m| m.name).collect())
}
