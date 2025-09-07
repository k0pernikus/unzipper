use std::io;
use std::path::Path;
use std::process::Command;

use crate::extractors::ArchiveExtractor;

pub struct RarExtractor;

impl ArchiveExtractor for RarExtractor {
    fn extract(&self, path: &Path, worker_id: usize) -> io::Result<()> {
        let (dest_dir, temp_renamed) = crate::prepare_dest_dir(path)?;
        let archive_arg: &Path = temp_renamed.as_ref().map(|p| p.as_path()).unwrap_or(path);
        let output = Command::new("7z")
            .arg("x")
            .arg("-y")
            .arg(format!("-o{}", dest_dir.display()))
            .arg(archive_arg)
            .output();
        match output {
            Ok(out) if out.status.success() => {
                println!("[Worker {}] Extracted rar {} via 7z", worker_id, path.display());
                Ok(())
            }
            Ok(out) => {
                eprintln!("[Worker {}] 7z failed for {}: exit {}", worker_id, path.display(), out.status);
                Err(io::Error::new(io::ErrorKind::Other, "7z extraction failed"))
            }
            Err(e) => {
                eprintln!("[Worker {}] 7z not available or failed to launch: {}", worker_id, e);
                Err(e)
            }
        }
    }
}
