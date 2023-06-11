use crate::DB;

#[tauri::command]
pub fn db_read(key: &str) -> String {
    if let Some(value) = DB.lock().unwrap().get::<String>(key) {
        return value
    }

    String::from("")
}

#[tauri::command]
pub fn db_write(key: &str, val: &str) -> bool {
    if let Ok(_) = DB.lock().unwrap().set(key, &val) {
        true
    } else {
        false
    }
}
