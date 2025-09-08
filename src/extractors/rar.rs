use std::io;
use std::path::Path;
use std::process::Command;

use crate::extractors::{ArchiveExtractor, log_start, log_done, log_error_status, log_error_launch};

pub struct RarExtractor;

impl ArchiveExtractor for RarExtractor {
    fn extract(&self, path: &Path, dest: &Path, worker_id: usize) -> io::Result<()> {
        log_start(worker_id, path, dest, "rar");
        let output = Command::new("7z")
            .arg("x")
            .arg("-y")
            .arg(format!("-o{}", dest.display()))
            .arg(path)
            .output();
        match output {
            Ok(out) if out.status.success() => {
                log_done(worker_id, path, "rar");
                Ok(())
            }
            Ok(out) => {
                log_error_status(worker_id, path, "7z", &out.status);
                Err(io::Error::new(io::ErrorKind::Other, "7z extraction failed"))
            }
            Err(e) => {
                log_error_launch(worker_id, "7z", &e);
                Err(e)
            }
        }
    }
}
