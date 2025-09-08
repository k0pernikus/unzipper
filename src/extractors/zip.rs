use std::fs;
use std::io;
use std::path::Path;
use zip::ZipArchive;
use crate::extractors::ArchiveExtractor;

pub struct ZipExtractor;

impl ArchiveExtractor for ZipExtractor {
    fn extract(&self, path: &Path, dest: &Path, worker_id: usize) -> io::Result<()> {
        println!("[Worker {}] Unzipping file: {} to {}", worker_id, path.display(), dest.display());
        {
            let file = fs::File::open(path)?;
            let mut archive = ZipArchive::new(file)?;
            for i in 0..archive.len() {
                let mut file = archive.by_index(i)?;
                println!("[Worker {}] Extracting: {}", worker_id, file.name());
                let outpath = match file.enclosed_name() {
                    Some(path) => dest.join(path),
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
        println!("[Worker {}] Successfully unzipped {}", worker_id, path.display());
        Ok(())
    }
}
