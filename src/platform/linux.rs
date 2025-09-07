use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub fn default_downloads_dir() -> PathBuf {
    linux_downloads_dir()
}

fn linux_downloads_dir() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| String::from("."));
    let config_path = Path::new(&home).join(".config").join("user-dirs.dirs");

    if let Ok(contents) = fs::read_to_string(&config_path) {
        for line in contents.lines() {
            let line = line.trim();
            if line.starts_with("XDG_DOWNLOAD_DIR") {
                if let Some(eq_idx) = line.find('=') {
                    let mut value = line[eq_idx + 1..].trim().trim_matches('"').to_string();
                    if value.contains("$HOME") {
                        value = value.replace("$HOME", &home);
                    }
                    let path = PathBuf::from(value);
                    return path;
                }
            }
        }
    }

    Path::new(&home).join("Downloads")
}
