use flate2::read::GzDecoder;
use std::fs;
use std::io;
use std::path::Path;
use tar::Archive as TarArchive;
use crate::extractors::{ArchiveExtractor, log_start, log_done};

pub struct TarGzExtractor;

impl ArchiveExtractor for TarGzExtractor {
    fn extract(&self, path: &Path, dest: &Path, worker_id: usize) -> io::Result<()> {
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        log_start(worker_id, path, dest, "tar/gz");
        {
            let file = fs::File::open(path)?;
            if file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz") || file_name.ends_with(".taz") {
                let gz = GzDecoder::new(file);
                let mut tar = TarArchive::new(gz);
                tar.unpack(dest)?;
            } else if file_name.ends_with(".tar") {
                let mut tar = TarArchive::new(file);
                tar.unpack(dest)?;
            } else if file_name.ends_with(".gz") {
                let mut gz = GzDecoder::new(file);
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let out_file_path = dest.join(stem);
                let mut out = fs::File::create(out_file_path)?;
                io::copy(&mut gz, &mut out)?;
            }
        }
        log_done(worker_id, path, "tar/gz");
        Ok(())
    }
}
