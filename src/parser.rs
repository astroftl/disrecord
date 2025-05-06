use std::path::PathBuf;
use std::sync::mpsc;
use tokio::fs::File;

pub struct ParseResult {
    input_path: PathBuf,
    output_result: Result<PathBuf, String>,
}

pub async fn parse(path: PathBuf) -> Result<mpsc::Receiver<ParseResult>, String> {
    if !path.exists() {
        error!("Path {} doesn't exist!", path.display());
        return Err(format!("Path {} doesn't exist!", path.display()));
    }

    let mut files_to_parse: Vec<PathBuf> = Vec::new();

    if path.is_file() {
        files_to_parse.push(path);
    } else if path.is_dir() {
        let dir_entries = path.read_dir();
        if let Err(e) = dir_entries {
            error!("Error reading directory: {e:?}");
            return Err(format!("Error reading directory: {e:?}"));
        }

        let dir_entries = dir_entries.unwrap();
        for entry in dir_entries {
            match entry {
                Ok(entry) => {
                    files_to_parse.push(entry.path());
                }
                Err(e) => {
                    error!("Error reading dir entry: {e:?}");
                }
            }
        }
    }

    let (parse_tx, parse_rx) = mpsc::channel();

    parse_files(files_to_parse, parse_tx).await;

    Ok(parse_rx)
}

async fn parse_files(paths: Vec<PathBuf>, parse_tx: mpsc::Sender<ParseResult>) {
    for path in paths {
        let sent_parse_tx = parse_tx.clone();
        tokio::spawn(async {
            parse_file(path, sent_parse_tx).await;
        });
    }
}

async fn parse_file(path: PathBuf, parse_tx: mpsc::Sender<ParseResult>) {
    let mut file = match File::open("foo.txt").await {
        Ok(file) => file,
        Err(e) => {
            error!("Error opening file: {e:?}");
            return;
        }
    };
    
    
}