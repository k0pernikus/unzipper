use std::env;
use std::path::{Path, PathBuf};

pub fn default_downloads_dir() -> PathBuf {
    let home = env::var("USERPROFILE").unwrap_or_else(|_| String::from("."));
    Path::new(&home).join("Downloads")
}
