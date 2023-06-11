use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug)]
pub struct AssistantCommand {
    pub path: PathBuf,
    pub commands: CommandsList,
}

#[derive(Deserialize, Debug)]
pub struct CommandsList {
    pub list: Vec<Config>,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub command: ConfigCommandSection,

    pub voice: ConfigVoiceSection,

    pub phrases: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct ConfigCommandSection {
    pub action: String,

    #[serde(default)]
    pub exe_path: String,

    #[serde(default)]
    pub exe_args: Vec<String>,

    #[serde(default)]
    pub cli_cmd: String,

    #[serde(default)]
    pub cli_args: Vec<String>
}

#[derive(Deserialize, Debug)]
pub struct ConfigVoiceSection {

    #[serde(default)]
    pub sounds: Vec<String>,
}
