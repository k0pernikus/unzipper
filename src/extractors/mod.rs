use std::io;
use std::path::Path;

pub trait ArchiveExtractor {
    fn extract(&self, path: &Path, dest: &Path, worker_id: usize) -> io::Result<()>;
}

#[inline]
pub fn log_extracting(worker_id: usize, entry_name: &str) {
    println!("[Worker {}] Extracting: {}", worker_id, entry_name);
}

#[inline]
pub fn log_start(worker_id: usize, src: &Path, dest: &Path, _kind: &str) {
    println!("[Worker {}] Unzipping file: {} to {}", worker_id, src.display(), dest.display());
}

#[inline]
pub fn log_done(worker_id: usize, src: &Path, _kind: &str) {
    println!("[Worker {}] Successfully unzipped {}", worker_id, src.display());
}

#[inline]
pub fn log_error_status(worker_id: usize, src: &Path, tool: &str, status: &std::process::ExitStatus) {
    eprintln!("[Worker {}] {} failed for {}: exit {}", worker_id, tool, src.display(), status);
}

#[inline]
pub fn log_error_launch(worker_id: usize, tool: &str, error: &dyn std::fmt::Display) {
    eprintln!("[Worker {}] {} not available or failed to launch: {}", worker_id, tool, error);
}

pub mod zip;
pub mod targz;
pub mod sevenz;
pub mod rar;
