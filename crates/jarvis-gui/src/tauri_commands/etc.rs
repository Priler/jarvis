use jarvis_core::{config, APP_LOG_DIR};

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command

#[tauri::command]
pub fn get_app_version() -> String {
    if let Some(res) = config::APP_VERSION {
        res.to_string()
    } else {
        String::from("error")
    }
}

#[tauri::command]
pub fn get_author_name() -> String {
    if let Some(res) = config::AUTHOR_NAME {
        res.to_string()
    } else {
        String::from("error")
    }
}

#[tauri::command]
pub fn get_repository_link() -> String {
    if let Some(res) = config::REPOSITORY_LINK {
        res.to_string()
    } else {
        String::from("error")
    }
}

#[tauri::command]
pub fn get_tg_official_link() -> String {
    if let Some(ver) = config::TG_OFFICIAL_LINK {
        ver.to_string()
    } else {
        String::from("error")
    }
}

#[tauri::command]
pub fn get_boosty_link() -> String {
    if let Some(ver) = config::SUPPORT_BOOSTY_LINK {
        ver.to_string()
    } else {
        String::from("error")
    }
}

#[tauri::command]
pub fn get_patreon_link() -> String {
    if let Some(ver) = config::SUPPORT_PATREON_LINK {
        ver.to_string()
    } else {
        String::from("error")
    }
}

#[tauri::command]
pub fn get_feedback_link() -> String {
    if let Some(res) = config::FEEDBACK_LINK {
        res.to_string()
    } else {
        String::from("error")
    }
}

#[tauri::command]
pub fn get_log_file_path() -> String {
    APP_LOG_DIR
        .get()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
