#[macro_use]
extern crate log;

mod discord;
mod commands;
mod recorder;
mod config;

use crate::config::{AppConfig, OutputConfig};
use fern::colors::{Color, ColoredLevelConfig};
use log::LevelFilter;
use recorder::recorder::Recorder;
use serenity::Client;
use serenity::all::ApplicationId;
use serenity::prelude::GatewayIntents;
use songbird::driver::DecodeMode;
use songbird::{Config, SerenityInit};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::{env, fs};

fn main() -> ExitCode {
    setup_logger();
    bot()
}

#[tokio::main]
async fn bot() -> ExitCode {
    let config_file = PathBuf::from(env::var("CONFIG_FILE").unwrap_or(String::from("config.toml")));
    let config: Arc<AppConfig> = match fs::read_to_string(config_file) {
        Ok(str) => {
            match toml::from_str(&str) {
                Ok(config) => Arc::new(config),
                Err(e) => {
                    error!("Error parsing config: {e:?}");
                    return ExitCode::FAILURE;
                }
            }
        },
        Err(e) => {
            error!("Error reading config: {e:?}");
            let bot_token = env::var("BOT_TOKEN").expect("Expected a BOT_TOKEN in the environment");
            let app_id: ApplicationId = env::var("APP_ID").expect("Expected an APP_ID in the environment")
                .parse().expect("APP_ID is not a valid ID");

            Arc::new(AppConfig {
                bot_token,
                app_id,
                record_dir: PathBuf::from(env::var("RECORD_DIR").unwrap_or(String::from("recordings"))),
                date_format: env::var("DATE_FORMAT").unwrap_or(String::from("%Y_%m_%d_%H_%M_%S")),
                default_output: OutputConfig {
                    embed_files: match env::var("EMBED_FILES") {
                        Ok(str) => {
                            match str.trim().to_lowercase().as_str() {
                                "false" | "no" | "0" => Some(false),
                                _ => Some(true) // anything that isn't false, no, or 0 is treated as truthy
                            }
                        }
                        Err(_) => Some(false)
                    },
                    post_command: env::var("POST_FILE_COMMAND").ok()
                },
                server_output: Default::default(),
            })
        }
    };



    let intents = GatewayIntents::non_privileged();

    let songbird_config = Config::default()
        .decode_mode(DecodeMode::Decrypt);

    let recorder = Arc::new(Recorder::new(config.clone()));

    let mut client = Client::builder(&config.bot_token, intents)
        .event_handler(discord::Events)
        .application_id(config.app_id)
        .register_songbird_from_config(songbird_config)
        .type_map_insert::<Recorder>(recorder)
        .await
        .expect("Error creating client!");

    info!("Starting Disrecord...");

    if let Err(why) = client.start_autosharded().await {
        error!("Client error: {:?}", why);
    }

    info!("Goodbye!");

    ExitCode::SUCCESS
}

fn setup_logger() {
    let colors_line = ColoredLevelConfig::new()
        .error(Color::BrightRed)
        .warn(Color::BrightYellow)
        .info(Color::BrightWhite)
        .debug(Color::White)
        .trace(Color::BrightBlack);

    let colors_level = colors_line.clone()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::BrightGreen)
        .debug(Color::BrightCyan)
        .trace(Color::Black);

    let log_level = if let Ok(level) = env::var("LOG_LEVEL") {
        match level.as_str() {
            "error" => LevelFilter::Error,
            "warn" => LevelFilter::Warn,
            "info" => LevelFilter::Info,
            "debug" => LevelFilter::Debug,
            "trace" => LevelFilter::Trace,
            _ => panic!("Unknown log level: {}", level),
        }
    } else {
        LevelFilter::Trace
    };

    let log_level_all = if let Ok(level) = env::var("LOG_LEVEL_ALL") {
        match level.as_str() {
            "error" => LevelFilter::Error,
            "warn" => LevelFilter::Warn,
            "info" => LevelFilter::Info,
            "debug" => LevelFilter::Debug,
            "trace" => LevelFilter::Trace,
            _ => panic!("Unknown log level: {}", level),
        }
    } else {
        LevelFilter::Warn
    };

    let mut dispatch = fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{color_line}[{date}][{target}][{level}{color_line}] {message}\x1B[0m",
                color_line = format_args!(
                    "\x1B[{}m",
                    colors_line.get_color(&record.level()).to_fg_str()
                ),
                date = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
                target = record.target(),
                level = colors_level.color(record.level()),
                message = message,
            ));
        })
        .level(log_level_all)
        .level_for("disrecord", log_level)
        .chain(std::io::stdout());

    match fern::log_file("disrecord.log") {
        Ok(logfile) => {
            dispatch = dispatch.chain(logfile);
        }
        Err(e) => {
            println!("Error setting up logger: {e}")
        }
    }

    dispatch
        .apply()
        .unwrap();
}