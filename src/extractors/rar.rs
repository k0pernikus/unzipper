use std::io;
use std::path::Path;
use std::process::Command;

use crate::extractors::{ArchiveExtractor, log_start, log_done, log_error_status, log_error_launch};

pub struct RarExtractor;

impl ArchiveExtractor for RarExtractor {
    fn extract(&self, path: &Path, worker_id: usize) -> io::Result<()> {
        let (dest_dir, temp_renamed) = crate::prepare_dest_dir(path)?;
        let archive_arg: &Path = temp_renamed.as_ref().map(|p| p.as_path()).unwrap_or(path);
        log_start(worker_id, path, &dest_dir, "rar");
        let output = Command::new("7z")
            .arg("x")
            .arg("-y")
            .arg(format!("-o{}", dest_dir.display()))
            .arg(archive_arg)
            .output();
        match output {
            Ok(out) if out.status.success() => {
                log_done(worker_id, path, "rar");
                if let Some(temp) = temp_renamed { let _ = std::fs::remove_file(temp); }
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
