#![feature(duration_millis_float)]

#[macro_use]
extern crate log;
mod discord;
mod commands;
mod recorder;
mod parser;

use crate::recorder::RecordConfig;
use clap::Parser;
use fern::colors::{Color, ColoredLevelConfig};
use log::LevelFilter;
use recorder::record_manager::RecordManager;
use serenity::all::ApplicationId;
use serenity::prelude::GatewayIntents;
use serenity::Client;
use songbird::driver::DecodeMode;
use songbird::{Config, SerenityInit};
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

///
#[derive(Parser, Debug)]
struct Args {
    /// Path to file or directory to post-process
    #[arg(short, long)]
    parse: Option<PathBuf>,
}

fn main() {
    dotenv::dotenv().ok();
    setup_logger();

    let args = Args::parse();

    match args.parse {
        None => {
            bot()
        }
        Some(path) => {
            parse(path)
        }
    }
}

#[tokio::main]
async fn bot() {
    let bot_token = env::var("BOT_TOKEN").expect("Expected a BOT_TOKEN in the environment");

    let app_id: ApplicationId = env::var("APP_ID").expect("Expected an APP_ID in the environment")
        .parse().expect("APP_ID is not a valid ID");

    let intents = GatewayIntents::non_privileged();

    let songbird_config = Config::default()
        .decode_mode(DecodeMode::Decrypt);

    let record_config = RecordConfig {
        base_dir: PathBuf::from("recordings"),
        subdir_fmt: "%Y_%m_%d_%H_%M_%S".to_string(),
    };

    let mut client = Client::builder(&bot_token, intents)
        .event_handler(discord::Events)
        .application_id(app_id)
        .register_songbird_from_config(songbird_config)
        .type_map_insert::<RecordManager>(Arc::new(RecordManager::new(record_config)))
        .await
        .expect("Error creating client!");

    info!("Starting Disrecord...");

    if let Err(why) = client.start_autosharded().await {
        error!("Client error: {:?}", why);
    }

    info!("Goodbye!")
}

#[tokio::main]
async fn parse(path: PathBuf) {
    info!("Parsing Disrecord output: {}", path.display());

    parser::parse(path).await;

    info!("Goodbye!")
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
                date = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
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