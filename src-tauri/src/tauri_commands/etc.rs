use crate::config::APP_VERSION;
use crate::config::AUTHOR_NAME;
use crate::config::REPOSITORY_LINK;

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command

#[tauri::command]
pub fn get_app_version() -> String {
    if let Some(ver) = APP_VERSION {
        ver.to_string()
    } else {
        String::from("error")
    }
}

#[tauri::command]
pub fn get_author_name() -> String {
    if let Some(ver) = AUTHOR_NAME {
        ver.to_string()
    } else {
        String::from("error")
    }
}

#[tauri::command]
pub fn get_repository_link() -> String {
    if let Some(ver) = REPOSITORY_LINK {
        ver.to_string()
    } else {
        String::from("error")
    }
}
