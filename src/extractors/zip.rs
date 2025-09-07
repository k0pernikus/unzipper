use std::fs;
use std::io;
use std::path::Path;
use zip::ZipArchive;

use crate::extractors::ArchiveExtractor;

pub struct ZipExtractor;

impl ArchiveExtractor for ZipExtractor {
    fn extract(&self, zip_path: &Path, worker_id: usize) -> io::Result<()> {
        let (dest_dir, temp_renamed) = crate::prepare_dest_dir(zip_path)?;
        println!("[Worker {}] Unzipping file: {} to {}", worker_id, zip_path.display(), dest_dir.display());
        {
            let file = fs::File::open(temp_renamed.as_ref().map(|p| p.as_path()).unwrap_or(zip_path))?;
            let mut archive = ZipArchive::new(file)?;
            for i in 0..archive.len() {
                let mut file = archive.by_index(i)?;
                println!("[Worker {}] Extracting: {}", worker_id, file.name());
                let outpath = match file.enclosed_name() {
                    Some(path) => dest_dir.join(path),
                    None => continue
                };
                if file.name().ends_with('/') {
                    fs::create_dir_all(&outpath)?;
                    continue;
                }
                if let Some(p) = outpath.parent() { if !p.exists() { fs::create_dir_all(p)?; } }
                let mut outfile = fs::File::create(&outpath)?;
                io::copy(&mut file, &mut outfile)?;
            }
        }
        println!("[Worker {}] Successfully unzipped {}", worker_id, zip_path.display());
        Ok(())
    }
}
