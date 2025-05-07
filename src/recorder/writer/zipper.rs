use std::path::PathBuf;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use serenity::all::GuildId;
use tokio::fs::{read_dir, File};
use tokio::io::AsyncReadExt;
use tokio::sync::oneshot::Sender;

async fn do_zip_files(directory: PathBuf, zip_name: String, guild_id: GuildId) -> Result<PathBuf, String> {
    let zip_path = directory.join(zip_name);
    debug!("[{guild_id}] Creating zip archive at {}", zip_path.display());

    let mut zip_file = match File::create(&zip_path).await {
        Ok(x) => x,
        Err(e) => {
            error!("[{guild_id}] Failed to create zip file {}: {e:?}", zip_path.display());
            return Err(format!("Failed to create zip file: {e}"));
        }
    };

    let mut zip_writer = ZipFileWriter::with_tokio(&mut zip_file);

    let mut dir_entries = match read_dir(&directory).await {
        Ok(x) => x,
        Err(e) => {
            error!("[{guild_id}] Failed to read recording directory {}: {e:?}", directory.display());
            return Err(format!("Failed to create zip archive: {e}"));
        }
    };

    while let Ok(Some(entry)) = dir_entries.next_entry().await {
        let path = entry.path();

        if path == zip_path {
            continue;
        }

        let file_name = match path.file_name() {
            Some(x) => x,
            None => {
                error!("[{guild_id}] Failed to get file name! (this should never happen)");
                continue
            }
        };

        let file_name = file_name.to_string_lossy().to_string();

        debug!("[{guild_id}] Adding {file_name} to zip archive...");

        let mut file = match File::open(&path).await {
            Ok(x) => x,
            Err(e) => {
                error!("[{guild_id}] Failed to open file for reading: {e:?}");
                continue
            }
        };

        let file_size = file.metadata().await.unwrap().len() as usize;

        trace!("[{guild_id}] Reading file {file_name} into buffer...");
        let mut buffer = Vec::with_capacity(file_size);
        if let Err(e) = file.read_to_end(&mut buffer).await {
            error!("[{guild_id}] Failed to read file: {e:?}");
            continue
        }

        trace!("[{guild_id}] Writing buffer into zip...");
        let builder = ZipEntryBuilder::new(file_name.into(), Compression::Deflate);
        if let Err(e) = zip_writer.write_entry_whole(builder, &buffer).await {
            error!("[{guild_id}] Failed to write to zip entry: {e:?}");
            continue
        }
    }

    trace!("[{guild_id}] Finalizing zip...");
    if let Err(e) = zip_writer.close().await {
        error!("[{guild_id}] Failed to finalize zip file: {e:?}");
        return Err(format!("Failed to finalize zip file: {e}"));
    }

    info!("[{guild_id}] Wrote zip: {}", zip_path.display());

    Ok(zip_path)
}

pub async fn zip_files(directory: PathBuf, zip_name: String, guild_id: GuildId, zip_tx: Sender<Result<PathBuf, String>>) {
    let res = do_zip_files(directory, zip_name, guild_id).await;
    if let Err(_) = zip_tx.send(res) {
        error!("[{guild_id}] Failed to send zip files result to the channel!");
    }
}