use flate2::read::GzDecoder;
use std::fs;
use std::io;
use std::path::Path;
use tar::Archive as TarArchive;

use crate::build_dest_dir_with_extension;
use crate::extractors::ArchiveExtractor;

pub struct TarGzExtractor;

impl ArchiveExtractor for TarGzExtractor {
    fn extract(&self, path: &Path, worker_id: usize) -> io::Result<()> {
        let dest_dir = build_dest_dir_with_extension(path);
        fs::create_dir_all(&dest_dir)?;
        let file = fs::File::open(path)?;
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz") || file_name.ends_with(".taz") {
            let gz = GzDecoder::new(file);
            let mut tar = TarArchive::new(gz);
            tar.unpack(&dest_dir)?;
        } else if file_name.ends_with(".tar") {
            let mut tar = TarArchive::new(file);
            tar.unpack(&dest_dir)?;
        } else if file_name.ends_with(".gz") {
            let mut gz = GzDecoder::new(file);
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let out_file_path = dest_dir.join(stem);
            let mut out = fs::File::create(out_file_path)?;
            io::copy(&mut gz, &mut out)?;
        }
        println!("[Worker {}] Extracted tar/gz {}", worker_id, path.display());
        Ok(())
    }
}
