use crate::config;
use crate::APP_LOG_DIR;

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
pub fn get_feedback_link() -> String {
    if let Some(res) = config::FEEDBACK_LINK {
        res.to_string()
    } else {
        String::from("error")
    }
}

#[tauri::command]
pub fn get_log_file_path() -> String {
    format!("{}", APP_LOG_DIR.lock().unwrap())
}