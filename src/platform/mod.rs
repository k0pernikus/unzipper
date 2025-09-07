use std::path::PathBuf;

#[cfg(windows)]
pub fn default_downloads_dir() -> PathBuf {
    windows::default_downloads_dir()
}

#[cfg(target_os = "macos")]
pub fn default_downloads_dir() -> PathBuf {
    macos::default_downloads_dir()
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn default_downloads_dir() -> PathBuf {
    linux::default_downloads_dir()
}

#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
