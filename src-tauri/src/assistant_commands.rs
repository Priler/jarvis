use rand::seq::SliceRandom;
use seqdiff::ratio;
use serde_yaml;
use std::path::Path;
use std::{fs, fs::File};
use log::{info, warn, error};

use core::time::Duration;
use std::path::PathBuf;
use std::process::{Command, Child};
// use tauri::Manager;

mod structs;
pub use structs::*;

use crate::config;
use crate::events;

pub fn parse_commands() -> Result<Vec<AssistantCommand>, String> {
    // collect commands
    let mut commands: Vec<AssistantCommand> = vec![];

    // read commands directories first
    if let Ok(cpaths) = fs::read_dir(config::COMMANDS_PATH) {
        for cpath in cpaths {
            // validate this command, check if required files exists
            let _cpath = cpath.unwrap().path();
            let cc_file = Path::new(&_cpath).join("command.yaml");

            if cc_file.exists() {
                // try parse config files
                let cc_reader = std::fs::File::open(&cc_file).unwrap();
                let cc_yaml: CommandsList;

                // try parse command.yaml
                match serde_yaml::from_reader::<File, CommandsList>(cc_reader) {
                    Ok(parse_result) => {
                        cc_yaml = parse_result;
                    },
                    Err(msg) => {
                        warn!("Can't parse {}, skipping ...\nCommand parse error is: {:?}", &cc_file.display(), msg);
                        eprintln!("Can't parse {}, skipping ...\nCommand parse error is: {:?}", &cc_file.display(), msg);
                        continue;
                    }
                }
                // everything seems to be Ok
                commands.push(AssistantCommand {
                    path: _cpath,
                    commands: cc_yaml,
                });
            }
        }

        if commands.len() > 0 {
            Ok(commands)
        } else {
            error!("No commands were found");
            Err("No commands were found".into())
        }
    } else {
        error!("Error reading commands directory");
        return Err("Error reading commands directory".into());
    }
}

pub fn fetch_command<'a>(
    phrase: &str,
    commands: &'a Vec<AssistantCommand>,
) -> Option<(&'a PathBuf, &'a Config)> {
    // result scmd
    let mut result_scmd: Option<(&PathBuf, &Config)> = None;
    let mut current_max_ratio = config::CMD_RATIO_THRESHOLD;

    // convert fetch phrase to sequence
    let fetch_phrase_chars = phrase.chars().collect::<Vec<_>>();

    // list all the commands
    for cmd in commands {
        // list all subcommands
        for scmd in &cmd.commands.list {
            // list all phrases in command
            for cmd_phrase in &scmd.phrases {
                // convert cmd phrase to sequence
                let cmd_phrase_chars = cmd_phrase.chars().collect::<Vec<_>>();

                // compare fetch phrase with cmd phrase
                let ratio = ratio(&fetch_phrase_chars, &cmd_phrase_chars);

                // return, if it fits the given threshold
                if ratio >= current_max_ratio {
                    result_scmd = Some((&cmd.path, &scmd));
                    current_max_ratio = ratio;
                    // println!("Ratio is: {}", ratio);
                    // return Some((&cmd.path, &scmd))
                }
            }
        }
    }

    if let Some((cmd_path, scmd)) = result_scmd {
        println!("Ratio is: {}", current_max_ratio);
        info!("CMD is: {cmd_path:?}, SCMD is: {scmd:?}, Ratio is: {}", current_max_ratio);
        Some((&cmd_path, &scmd))
    } else {
        None
    }
}

pub fn execute_exe(exe: &str, args: &Vec<String>) -> std::io::Result<Child> {
    Command::new(exe).args(args).spawn()
}

pub fn execute_cli(cmd: &str, args: &Vec<String>) -> std::io::Result<Child> {

    println!("Spawning cmd as: cmd /C {} {:?}", cmd, args);

    if cfg!(target_os = "windows") {
        Command::new("cmd")
                .arg("/C")
                .arg(cmd)
                .args(args)
                .spawn()
    } else {
        Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .args(args)
                .spawn()
    }
}

pub fn execute_command(
    cmd_path: &PathBuf,
    cmd_config: &Config,
    app_handle: &tauri::AppHandle,
) -> Result<bool, String> {
    match cmd_config.command.action.as_str() {
        "voice" => {
            // VOICE command type
            let random_cmd_sound = cmd_config
                .voice
                .sounds
                .choose(&mut rand::thread_rng())
                .unwrap();
            events::play(random_cmd_sound, app_handle);

            Ok(true)
        }
        "ahk" => {
            // AutoHotkey command type
            let exe_path_absolute = Path::new(&cmd_config.command.exe_path);
            let exe_path_local = Path::new(&cmd_path).join(&cmd_config.command.exe_path);

            if let Ok(_) = execute_exe(
                if exe_path_absolute.exists() {
                    exe_path_absolute.to_str().unwrap()
                } else {
                    exe_path_local.to_str().unwrap()
                },
                &cmd_config.command.exe_args,
            ) {
                let random_cmd_sound = cmd_config
                    .voice
                    .sounds
                    .choose(&mut rand::thread_rng())
                    .unwrap();
                events::play(random_cmd_sound, app_handle);

                Ok(true)
            } else {
                error!("AHK process spawn error (does exe path is valid?)");
                Err("AHK process spawn error (does exe path is valid?)".into())
            }
        }
        "cli" => {
            // CLI command type
            let cli_cmd = &cmd_config.command.cli_cmd;

            match execute_cli(
                cli_cmd,
                &cmd_config.command.cli_args,
            ) {
                    Ok(_) => {
                        let random_cmd_sound = cmd_config
                        .voice
                        .sounds
                        .choose(&mut rand::thread_rng())
                        .unwrap();
                    events::play(random_cmd_sound, app_handle);

                    Ok(true)
                },
                Err(msg) => {
                    error!("CLI command error ({})", msg);
                    Err(format!("Shell command error ({})", msg).into())
                }
            }
        }
        "terminate" => {
            // TERMINATE command type
            let random_cmd_sound = cmd_config
                .voice
                .sounds
                .choose(&mut rand::thread_rng())
                .unwrap();
            events::play(random_cmd_sound, app_handle);

            std::thread::sleep(Duration::from_secs(2));
            std::process::exit(0);
        }
        "stop_chaining" => {
            // STOP_CHAINING command type
            let random_cmd_sound = cmd_config
                .voice
                .sounds
                .choose(&mut rand::thread_rng())
                .unwrap();
            events::play(random_cmd_sound, app_handle);

            Ok(false)
        }
        _ => {
            error!("Command type unknown");
            Err("Command type unknown".into())
        },
    }
}
