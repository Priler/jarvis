use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use std::time::{Duration, Instant};
use std::process::{Child, Command};

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use seqdiff::ratio;

mod structs;
pub use structs::*;

use crate::{config, i18n, APP_DIR};

// CLI commands that always require confirmation regardless of the `confirm` flag.
// Includes destructive system tools AND shell interpreters that can run arbitrary code.
const ALWAYS_CONFIRM_CMDS: &[&str] = &[
    // destructive system tools
    "shutdown", "format", "diskpart", "reg", "del", "rmdir", "rd", "cipher",
    // shell interpreters — can execute arbitrary code via arguments
    "cmd", "powershell", "pwsh", "sh", "bash", "zsh", "fish",
    "wscript", "cscript", "mshta", "rundll32", "regsvr32",
];

pub struct PendingConfirm {
    pub id: String,
    pub cmd_path: PathBuf,
    pub cmd: JCommand,
    pub created_at: Instant,
}

static PENDING_CONFIRM: Lazy<Mutex<Option<PendingConfirm>>> = Lazy::new(|| Mutex::new(None));

pub fn requires_confirmation(cmd: &JCommand) -> bool {
    if cmd.cmd_type != "cli" {
        return false;
    }
    let cli_lower = cmd.cli_cmd.to_lowercase();
    cmd.confirm
        || ALWAYS_CONFIRM_CMDS
            .iter()
            .any(|&c| cli_lower == c || cli_lower.starts_with(&format!("{} ", c)))
}

pub fn store_pending_command(path: &PathBuf, cmd: &JCommand) {
    *PENDING_CONFIRM.lock() = Some(PendingConfirm {
        id: cmd.id.clone(),
        cmd_path: path.clone(),
        cmd: cmd.clone(),
        created_at: Instant::now(),
    });
}

pub fn take_pending_command() -> Option<PendingConfirm> {
    PENDING_CONFIRM.lock().take()
}

/// Expire a pending confirmation that has been waiting longer than `max_age_s` seconds.
/// Called by the watchdog every interval to GC abandoned confirmations.
pub fn expire_pending_confirm(max_age_s: u64) {
    let mut guard = PENDING_CONFIRM.lock();
    if let Some(ref pending) = *guard {
        let age_s = pending.created_at.elapsed().as_secs();
        if age_s >= max_age_s {
            warn!(
                "[SESSION_GC] Expired stale pending confirm id='{}' age={}s",
                pending.id, age_s
            );
            *guard = None;
        }
    }
}

#[cfg(feature = "lua")]
use crate::lua::{self, SandboxLevel, CommandContext};

pub fn parse_commands() -> Result<Vec<JCommandsList>, String> {
    let mut commands: Vec<JCommandsList> = Vec::new();

    let commands_path = APP_DIR.join(config::COMMANDS_PATH);
    let cmd_dirs = fs::read_dir(&commands_path)
        .map_err(|e| format!("Error reading commands directory {:?}: {}", commands_path, e))?;

    for entry in cmd_dirs.flatten() {
        let cmd_path = entry.path();
        let toml_file = cmd_path.join("command.toml");
        
        if !toml_file.exists() {
            continue;
        }
        
        let content = match fs::read_to_string(&toml_file) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read {}: {}", toml_file.display(), e);
                continue;
            }
        };

        let file: JCommandsList = match toml::from_str(&content) {
            Ok(f) => f,
            Err(e) => {
                warn!("Failed to parse {}: {}", toml_file.display(), e);
                continue;
            }
        };

        commands.push(JCommandsList {
            path: cmd_path,
            commands: file.commands,
        });
    }

    if commands.is_empty() {
        Err("No commands found".into())
    } else {
        info!("Loaded {} command pack(s)", commands.len());
        Ok(commands)
    }
}


pub fn commands_hash(commands: &[JCommandsList]) -> String {
    use sha2::{Sha256, Digest};
    
    let mut hasher = Sha256::new();
    
    let lang = i18n::get_language();
    hasher.update(lang.as_bytes());
    hasher.update(b"|");

    // collect all command ids and phrases for current language, sorted
    let mut all_data: Vec<(&str, _)> = commands.iter()
        .flat_map(|ac| ac.commands.iter().map(|c| (c.id.as_str(), c.get_phrases(&lang))))
        .collect();
    all_data.sort_by_key(|(id, _)| *id);
    
    for (id, phrases) in all_data {
        hasher.update(id.as_bytes());
        for phrase in phrases.iter() {
            hasher.update(phrase.as_bytes());
        }
    }
    
    format!("{:x}", hasher.finalize())
}


pub fn fetch_command<'a>(
    phrase: &str,
    commands: &'a [JCommandsList],
) -> Option<(&'a PathBuf, &'a JCommand)> {
    let lang = i18n::get_language();

    let phrase = phrase.trim().to_lowercase();
    if phrase.is_empty() {
        return None;
    }

    let phrase_chars: Vec<char> = phrase.chars().collect();
    let phrase_words: Vec<&str> = phrase.split_whitespace().collect();

    let mut result: Option<(&PathBuf, &JCommand)> = None;
    let mut best_score = config::CMD_RATIO_THRESHOLD;

    for cmd_list in commands {
        for cmd in &cmd_list.commands {
            let cmd_phrases = cmd.get_phrases(&lang);
            
            for cmd_phrase in cmd_phrases.iter() {
                let cmd_phrase_lower = cmd_phrase.trim().to_lowercase();
                let cmd_phrase_chars: Vec<char> = cmd_phrase_lower.chars().collect();
                
                // character-level similarity
                let char_ratio = ratio(&phrase_chars, &cmd_phrase_chars);
                
                // word-level similarity
                let cmd_words: Vec<&str> = cmd_phrase_lower.split_whitespace().collect();
                let word_score = word_overlap_score(&phrase_words, &cmd_words);
                
                // combined score
                let score = (char_ratio * 0.6) + (word_score * 0.4);
                
                // early exit on perfect match
                if score >= 99.0 {
                    debug!("Perfect match: '{}' -> '{}'", phrase, cmd_phrase_lower);
                    return Some((&cmd_list.path, cmd));
                }
                
                if score > best_score {
                    best_score = score;
                    result = Some((&cmd_list.path, cmd));
                }
            }
        }
    }

    if let Some((_, cmd)) = result {
        info!("Fuzzy match: '{}' -> cmd '{}' (score: {:.1}%)", phrase, cmd.id, best_score);
    } else {
        debug!("No match for '{}' (best: {:.1}%)", phrase, best_score);
    }
    
    result
}


fn word_overlap_score(input_words: &[&str], cmd_words: &[&str]) -> f64 {
    if input_words.is_empty() || cmd_words.is_empty() {
        return 0.0;
    }

    let mut matched = 0.0;
    
    // pre-compute cmd word chars to avoid repeated allocations
    let cmd_word_chars: Vec<Vec<char>> = cmd_words
        .iter()
        .map(|w| w.chars().collect())
        .collect();
    
    for input_word in input_words {
        let input_chars: Vec<char> = input_word.chars().collect();
        
        let best_word_match = cmd_word_chars
            .iter()
            .map(|cw| ratio(&input_chars, cw))
            .fold(0.0_f64, f64::max);
        
        if best_word_match > 70.0 {
            matched += best_word_match / 100.0;
        }
    }

    let max_words = input_words.len().max(cmd_words.len()) as f64;
    (matched / max_words) * 100.0
}




pub fn execute_exe(exe: &str, args: &[String]) -> std::io::Result<Child> {
    Command::new(exe).args(args).spawn()
}

pub fn execute_cli(cmd: &str, args: &[String]) -> std::io::Result<Child> {
    debug!("Spawning: {} {:?}", cmd, args);
    Command::new(cmd).args(args).spawn()
}

pub fn execute_command(cmd_path: &PathBuf, cmd_config: &JCommand, _phrase: Option<&str>, _slots: Option<&HashMap<String, SlotValue>>) -> Result<bool, String> {
    // execute command by the type
    match cmd_config.cmd_type.as_str() {

        // BRUH
        "voice" => Ok(cmd_config.chain),

        // LUA command
        #[cfg(feature = "lua")]
        "lua" => {
            execute_lua_command(cmd_path, cmd_config, _phrase, _slots)
        }

        // AutoHotkey command
        // @TODO: Consider adding ahk source files execution?
        "ahk" => {
            // SEC-4: reject absolute exe_path and traversal sequences so command.toml
            // cannot redirect to arbitrary system binaries (e.g. cmd.exe, powershell.exe).
            let ep = &cmd_config.exe_path;
            if Path::new(ep).is_absolute() || ep.contains("..") {
                return Err(format!(
                    "AHK exe_path must be relative and within the command folder: '{}'", ep
                ));
            }
            let exe_path = cmd_path.join(ep);
            execute_exe(exe_path.to_str().unwrap(), &cmd_config.exe_args)
                .map(|_| cmd_config.chain)
                .map_err(|e| format!("AHK process spawn error: {}", e))
        }

        "cli" => {
            execute_cli(&cmd_config.cli_cmd, &cmd_config.cli_args)
                .map(|_| cmd_config.chain)
                .map_err(|e| format!("CLI command error: {}", e))
        }
        
        // TERMINATOR command (T1000)
        "terminate" => {
            std::thread::sleep(Duration::from_secs(2));
            std::process::exit(0);
        }
        
        // STOP CHANING
        "stop_chaining" => Ok(false),

        // other
        _ => {
            error!("Command type unknown: {}", cmd_config.cmd_type);
            Err(format!("Command type unknown: {}", cmd_config.cmd_type).into())
        }
    }
}

// look up a command by its ID
pub fn get_command_by_id<'a>(
    commands: &'a [JCommandsList],
    id: &str,
) -> Option<(&'a PathBuf, &'a JCommand)> {
    for cmd_list in commands {
        for cmd in &cmd_list.commands {
            if cmd.id == id {
                return Some((&cmd_list.path, cmd));
            }
        }
    }
    None
}

pub fn list_paths(commands: &[JCommandsList]) -> Vec<&Path> {
    commands.iter().map(|x| x.path.as_path()).collect()
}

#[cfg(feature = "lua")]
fn execute_lua_command(
    cmd_path: &PathBuf,
    cmd_config: &JCommand,
    phrase: Option<&str>,
    slots: Option<&HashMap<String, SlotValue>>
) -> Result<bool, String> {
    // get script path

    let script_name = if cmd_config.script.is_empty() {
        "script.lua"
    } else {
        &cmd_config.script
    };
    
    let script_path = cmd_path.join(script_name);
    
    if !script_path.exists() {
        return Err(format!("Lua script not found: {}", script_path.display()));
    }
    
    // parse sandbox level
    let sandbox = SandboxLevel::from_str(&cmd_config.sandbox);

    // create context
    let context = CommandContext {
        phrase: phrase.unwrap_or("").to_string(),
        command_id: cmd_config.id.clone(),
        command_path: cmd_path.clone(),
        language: i18n::get_language(),
        slots: slots.map(|s| s.clone()),
    };
    
    // get timeout
    let timeout = Duration::from_millis(cmd_config.timeout);
    
    info!("Executing Lua command: {} (sandbox: {:?}, timeout: {:?})", 
          cmd_config.id, sandbox, timeout);
    
    // execute
    match lua::execute(&script_path, context, sandbox, timeout) {
        Ok(result) => {
            info!("Lua command {} completed (chain: {})", cmd_config.id, result.chain);
            Ok(result.chain)
        }
        Err(e) => {
            error!("Lua command {} failed: {}", cmd_config.id, e);
            Err(e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli_cmd(cli_cmd: &str, confirm: bool) -> JCommand {
        serde_json::from_str(&format!(
            r#"{{"id":"t","type":"cli","cli_cmd":"{cli_cmd}","confirm":{confirm}}}"#
        )).unwrap()
    }

    fn voice_cmd(id: &str, phrase: &str) -> JCommandsList {
        let json = format!(
            r#"{{"commands":[{{"id":"{id}","type":"voice","phrases":{{"en":["{phrase}"]}}}}]}}"#
        );
        let mut list: JCommandsList = serde_json::from_str(&json).unwrap();
        list.path = PathBuf::from("/tmp");
        list
    }

    // --- requires_confirmation ---

    #[test]
    fn non_cli_type_never_needs_confirmation() {
        let cmd: JCommand = serde_json::from_str(r#"{"id":"t","type":"voice"}"#).unwrap();
        assert!(!requires_confirmation(&cmd));
    }

    #[test]
    fn safe_cli_cmd_no_confirmation() {
        assert!(!requires_confirmation(&cli_cmd("echo", false)));
    }

    #[test]
    fn dangerous_cmd_shutdown_needs_confirmation() {
        assert!(requires_confirmation(&cli_cmd("shutdown", false)));
    }

    #[test]
    fn dangerous_cmd_case_insensitive() {
        assert!(requires_confirmation(&cli_cmd("SHUTDOWN", false)));
    }

    #[test]
    fn dangerous_cmd_with_args_needs_confirmation() {
        assert!(requires_confirmation(&cli_cmd("shutdown /s /f /t 0", false)));
    }

    #[test]
    fn confirm_flag_forces_confirmation_for_safe_cmd() {
        assert!(requires_confirmation(&cli_cmd("notepad", true)));
    }

    #[test]
    fn del_cmd_exact_match_needs_confirmation() {
        assert!(requires_confirmation(&cli_cmd("del", false)));
    }

    #[test]
    fn cmd_starting_with_dangerous_prefix_but_no_space_is_safe() {
        // "deleteme" is not the same as "del" and does not start with "del "
        assert!(!requires_confirmation(&cli_cmd("deleteme", false)));
    }

    #[test]
    fn shell_interpreters_always_need_confirmation() {
        for prog in &["cmd", "powershell", "pwsh", "sh", "bash", "wscript", "cscript", "mshta", "rundll32"] {
            assert!(requires_confirmation(&cli_cmd(prog, false)), "{prog} should require confirmation");
        }
    }

    #[test]
    fn shell_interpreter_case_insensitive() {
        assert!(requires_confirmation(&cli_cmd("PowerShell", false)));
        assert!(requires_confirmation(&cli_cmd("CMD", false)));
    }

    // --- fetch_command ---

    #[test]
    fn empty_phrase_returns_none() {
        let lists = [voice_cmd("greet", "say hello")];
        assert!(fetch_command("", &lists).is_none());
    }

    #[test]
    fn whitespace_only_phrase_returns_none() {
        let lists = [voice_cmd("greet", "say hello")];
        assert!(fetch_command("   ", &lists).is_none());
    }

    #[test]
    fn no_commands_returns_none() {
        assert!(fetch_command("say hello", &[]).is_none());
    }

    #[test]
    fn exact_phrase_match_returns_command() {
        let lists = [voice_cmd("greet", "say hello")];
        let result = fetch_command("say hello", &lists);
        assert!(result.is_some());
        assert_eq!(result.unwrap().1.id, "greet");
    }

    #[test]
    fn phrase_match_is_case_insensitive() {
        let lists = [voice_cmd("greet", "say hello")];
        let result = fetch_command("SAY HELLO", &lists);
        assert!(result.is_some());
    }

    #[test]
    fn completely_unrelated_phrase_returns_none() {
        let lists = [voice_cmd("greet", "say hello")];
        assert!(fetch_command("xyzzy frobozzle quux", &lists).is_none());
    }

    #[test]
    fn best_of_multiple_commands_is_returned() {
        let json = r#"{"commands":[
            {"id":"a","type":"voice","phrases":{"en":["turn on the lights"]}},
            {"id":"b","type":"voice","phrases":{"en":["play some music"]}}
        ]}"#;
        let mut list: JCommandsList = serde_json::from_str(json).unwrap();
        list.path = PathBuf::from("/tmp");
        let lists = [list];
        let result = fetch_command("play some music", &lists);
        assert!(result.is_some());
        assert_eq!(result.unwrap().1.id, "b");
    }
}