use jarvis_core::commands::{self, JCommand, JCommandsList};
use once_cell::sync::Lazy;

static COMMANDS: Lazy<Vec<JCommandsList>> =
    Lazy::new(|| commands::parse_commands().unwrap_or_default());

#[tauri::command]
pub fn get_commands_count() -> usize {
    COMMANDS.iter().map(|list| list.commands.len()).sum()
}

#[tauri::command]
pub fn get_commands_list() -> Vec<JCommand> {
    COMMANDS
        .iter()
        .flat_map(|list| list.commands.clone())
        .collect()
}
