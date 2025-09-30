use std::path::PathBuf;
use std::process::Stdio;
use serenity::all::GuildId;
use tokio::sync::oneshot::Sender;
use tokio::process::Command;

pub async fn do_mix_files(input_files: &Vec<PathBuf>, directory: PathBuf, output_name: String, guild_id: GuildId) -> Result<PathBuf, String> {
    if input_files.is_empty() {
        error!("[{guild_id}] No input files provided.");
        return Err("No input files provided.".to_string());
    }

    let mut ffmpeg_args = vec![];
    let mut input_refs = vec![];

    for (i, file) in input_files.iter().enumerate() {
        ffmpeg_args.push("-i".to_string());
        ffmpeg_args.push(file.to_string_lossy().to_string());
        input_refs.push(format!("[{}:a]", i));
    }

    let filter_complex = format!(
        "{}amix=inputs={}:normalize=0",
        input_refs.join(""),
        input_files.len()
    );

    let output_path = directory.join(&output_name);

    ffmpeg_args.extend([
        "-filter_complex".to_string(),
        filter_complex,
        output_path.to_string_lossy().to_string(),
    ]);

    info!("[{guild_id}] Invoking ffmpeg with args: {ffmpeg_args:?}");

    let status = Command::new("ffmpeg")
        .args(ffmpeg_args)
        .stdin(Stdio::null())
        .status()
        .await
        .map_err(|e| {
            error!("[{guild_id}] Failed to run ffmpeg: {e:?}");
            format!("Failed to run ffmpeg: {e}")
        })?;

    if !status.success() {
        error!("[{guild_id}] ffmpeg exited with code {}", status.code().unwrap_or(-1));
        return Err(format!("ffmpeg exited with code {}", status.code().unwrap_or(-1)));
    }

    Ok(output_path)
}


pub async fn mix_files(input_files: &Vec<PathBuf>, directory: PathBuf, output_name: String, guild_id: GuildId, mix_tx: Sender<Result<PathBuf, String>>) {
    let res = do_mix_files(input_files, directory, output_name, guild_id).await;
    if let Err(_) = mix_tx.send(res) {
        error!("[{guild_id}] Failed to send mix files result to the channel!");
    }
}