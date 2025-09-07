use std::fs;
use std::io;
use std::path::Path;

use crate::extractors::ArchiveExtractor;

pub struct SevenZExtractor;

impl ArchiveExtractor for SevenZExtractor {
    fn extract(&self, path: &Path, worker_id: usize) -> io::Result<()> {
        let (dest_dir, temp_renamed) = crate::prepare_dest_dir(path)?;
        crate::wait_until_stable(path, 5, std::time::Duration::from_millis(300))?;
        {
            let mut sz = sevenz_rust::SevenZReader::open(
                temp_renamed.as_ref().map(|p| p.as_path()).unwrap_or(path),
                sevenz_rust::Password::empty(),
            )
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
            sz.for_each_entries(|entry, mut reader| {
                let name = entry.name();
                let out = dest_dir.join(name);
                if !out.starts_with(&dest_dir) { return Err(io::Error::new(io::ErrorKind::Other, "Invalid entry path").into()); }
                if entry.is_directory() {
                    let _ = fs::create_dir_all(&out);
                    return Ok(true);
                }
                if let Some(p) = out.parent() { let _ = fs::create_dir_all(p); }
                if out.exists() {
                    if let Ok(perms) = fs::metadata(&out).and_then(|m| Ok(m.permissions())) {
                        if perms.readonly() {
                            let mut p = perms;
                            p.set_readonly(false);
                            let _ = fs::set_permissions(&out, p);
                        }
                    }
                }
                let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).open(&out)?;
                io::copy(&mut reader, &mut f)?;
                Ok(true)
            })
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        }
        println!("[Worker {}] Extracted 7z {}", worker_id, path.display());
        Ok(())
    }
}
