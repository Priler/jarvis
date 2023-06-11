use simple_log::LogConfigBuilder;

use crate::config;
use crate::APP_LOG_DIR;

pub fn init_logging() -> Result<(), String> {
    // configure logging
    let config = LogConfigBuilder::builder()
        .path(format!("{}/{}", APP_LOG_DIR.get().unwrap().display(), config::LOG_FILE_NAME))
        .size(1 * 100)
        .roll_count(10)
        .time_format("%Y-%m-%d %H:%M:%S.%f") //E.g:%H:%M:%S.%f
        .level("debug")
        .output_file()
        .output_console()
        .build();

    simple_log::new(config)?;

    Ok(())
}