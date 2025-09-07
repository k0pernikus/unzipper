use clap::Parser;
use notify::{EventKind, RecursiveMode, Result, Watcher};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use walkdir::WalkDir;

mod platform;
mod extractors;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short = 'p', long)]
    watch_path: Option<PathBuf>,
}

pub(crate) fn wait_until_stable(path: &Path, attempts: usize, delay: std::time::Duration) -> io::Result<()> {
    let mut prev_len = None;
    for _ in 0..attempts {
        match fs::metadata(path) {
            Ok(m) => {
                let len = m.len();
                if let Some(p) = prev_len { if p == len { return Ok(()); } }
                prev_len = Some(len);
            }
            Err(e) => {
                let _ = e.kind();
            }
        }
        std::thread::sleep(delay);
    }
    Ok(())
}


pub(crate) fn build_dest_dir_with_extension(path: &Path) -> PathBuf {
    let parent_dir = path.parent().unwrap_or_else(|| Path::new(""));
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let sanitized: String = file_name.chars().filter(|c| !"<>:\"/\\|?*".contains(*c)).collect();
    parent_dir.join(sanitized)
}

// Ensures the destination directory for extraction is available.
// If the archive file itself would conflict with the destination directory name
// (e.g., C:\path\file.zip and dest dir C:\path\file.zip), temporarily rename
// the archive, create the directory, and return the temp path to optionally
// restore later.
pub(crate) fn prepare_dest_dir(path: &Path) -> io::Result<(PathBuf, Option<PathBuf>)> {
    let dest_dir = build_dest_dir_with_extension(path);

    // If the destination directory already exists, we're fine.
    if dest_dir.is_dir() {
        return Ok((dest_dir, None));
    }

    // If a file exists at the dest path (likely the archive), we need to move it.
    if dest_dir.exists() {
        // Compute a temporary name for the archive file in the same directory.
        let mut tmp_name = dest_dir
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| format!("{}.__extracting__", s))
            .unwrap_or_else(|| String::from("__extracting__"));
        // Avoid collisions
        let mut tmp_path = dest_dir.parent().unwrap_or_else(|| Path::new("")) .join(&tmp_name);
        let mut counter = 1;
        while tmp_path.exists() {
            tmp_name = format!("{}.__extracting__{}", dest_dir.file_name().and_then(|s| s.to_str()).unwrap_or("archive"), counter);
            tmp_path = dest_dir.parent().unwrap_or_else(|| Path::new("")) .join(&tmp_name);
            counter += 1;
        }
        // Move the archive away, create the dir, and return the temp path.
        fs::rename(path, &tmp_path)?;
        fs::create_dir_all(&dest_dir)?;
        return Ok((dest_dir, Some(tmp_path)));
    }

    // Normal case: create the destination directory.
    fs::create_dir_all(&dest_dir)?;
    Ok((dest_dir, None))
}

fn unzip_file(zip_path: &Path, worker_id: usize) -> io::Result<()> {
    use crate::extractors::{zip::ZipExtractor, ArchiveExtractor};
    ZipExtractor.extract(zip_path, worker_id)
}

fn is_temp_file_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".crdownload") || lower.ends_with(".part") || lower.ends_with(".tmp")
}

fn process_file(path: &Path, worker_id: usize) {
    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
        if is_temp_file_name(name) { return; }
    }
    if let Err(e) = wait_until_stable(path, 5, std::time::Duration::from_millis(300)) {
        eprintln!("[Worker {}] Skipping {} due to stability check error: {}", worker_id, path.display(), e);
        return;
    }
    let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());
    let ext = match ext {
        Some(e) => e,
        None => return
    };

    match ext.as_str() {
        "zip" => {
            println!("[Worker {}] Processing zip file: {}", worker_id, path.display());
            if let Err(e) = unzip_file(path, worker_id) {
                eprintln!("[Worker {}] Error unzipping {}: {}", worker_id, path.display(), e);
                return;
            }
        }
        "tar" | "tgz" | "gz" | "tar.gz" | "taz" => {
            println!("[Worker {}] Processing tar/gz file: {}", worker_id, path.display());
            if let Err(e) = untar_or_gz(path, worker_id) {
                eprintln!("[Worker {}] Error extracting {}: {}", worker_id, path.display(), e);
                return;
            }
        }
        "7z" => {
            println!("[Worker {}] Processing 7z file: {}", worker_id, path.display());
            if let Err(e) = extract_7z(path, worker_id) {
                eprintln!("[Worker {}] Error extracting {}: {}", worker_id, path.display(), e);
                return;
            }
        }
        "rar" => {
            println!("[Worker {}] Processing rar file: {}", worker_id, path.display());
            if let Err(e) = extract_rar(path, worker_id) {
                eprintln!("[Worker {}] Error extracting {}: {}", worker_id, path.display(), e);
                return;
            }
        }
        _ => return,
    }

    // Attempt to delete the original archive. In our extraction flow, we may have
    // temporarily renamed the archive to free the destination directory path.
    // If the original file path is gone or blocked by a directory, try to clean
    // up any temp "__extracting__" files we might have created.
    match fs::remove_file(path) {
        Ok(_) => {
            println!("[Worker {}] Successfully deleted original archive: {}", worker_id, path.display());
        }
        Err(e) => {
            use std::io::ErrorKind;
            // NotFound: file may have been moved to a temp name.
            // PermissionDenied: the path may now be a directory; we still want to remove temp.
            if e.kind() == ErrorKind::NotFound || e.kind() == ErrorKind::PermissionDenied {
                if let Some(parent) = path.parent() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        let prefix = format!("{}.__extracting__", name);
                        if let Ok(entries) = fs::read_dir(parent) {
                            for entry in entries.flatten() {
                                if let Some(fname) = entry.file_name().to_str() {
                                    if fname.starts_with(&prefix) {
                                        let _ = fs::remove_file(entry.path());
                                    }
                                }
                            }
                        }
                    }
                }
                println!("[Worker {}] Original archive already moved/absent. Clean-up complete for {}.", worker_id, path.display());
            } else {
                eprintln!("[Worker {}] Error deleting {}: {}", worker_id, path.display(), e);
            }
        }
    }
}

fn untar_or_gz(path: &Path, worker_id: usize) -> io::Result<()> {
    use crate::extractors::{targz::TarGzExtractor, ArchiveExtractor};
    TarGzExtractor.extract(path, worker_id)
}

fn extract_7z(path: &Path, worker_id: usize) -> io::Result<()> {
    use crate::extractors::{sevenz::SevenZExtractor, ArchiveExtractor};
    SevenZExtractor.extract(path, worker_id)
}

fn extract_rar(path: &Path, worker_id: usize) -> io::Result<()> {
    use crate::extractors::{rar::RarExtractor, ArchiveExtractor};
    RarExtractor.extract(path, worker_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir() -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!("unzipper_test_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn test_is_temp_file_name() {
        assert!(is_temp_file_name("file.part"));
        assert!(is_temp_file_name("file.tmp"));
        assert!(is_temp_file_name("Some.CRDOWNLOAD"));
        assert!(!is_temp_file_name("archive.7z"));
        assert!(!is_temp_file_name("normal.zip"));
    }

    #[test]
    fn test_wait_until_stable_on_existing_file() {
        let mut p = std::env::temp_dir();
        p.push(format!("unzipper_test_{}_{}.tmp", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        {
            let mut f = std::fs::File::create(&p).expect("create temp file");
            writeln!(f, "hello").unwrap();
        }
        let res = wait_until_stable(&p, 3, std::time::Duration::from_millis(50));
        std::fs::remove_file(&p).ok();
        assert!(res.is_ok());
    }

    fn create_sample_zip(dir: &Path) -> PathBuf {
        let zip_path = dir.join("sample.zip");
        let file_path = dir.join("inner.txt");
        std::fs::write(&file_path, b"hi").unwrap();
        let f = std::fs::File::create(&zip_path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let options = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        z.start_file("inner.txt", options).unwrap();
        z.write_all(b"hi").unwrap();
        z.finish().unwrap();
        zip_path
    }

    #[test]
    fn test_unzip_file_extracts() {
        let td = temp_dir();
        let zip_path = create_sample_zip(&td);
        let res = unzip_file(&zip_path, 0);
        assert!(res.is_ok());
        let extracted_dir = td.join("sample.zip");
        assert!(extracted_dir.exists());
        let inner = extracted_dir.join("inner.txt");
        assert_eq!(std::fs::read_to_string(inner).unwrap(), "hi");
        std::fs::remove_dir_all(&td).ok();
    }

    #[test]
    fn test_process_file_zip_extracts_and_deletes_archive() {
        let td = temp_dir();
        let zip_path = create_sample_zip(&td);
        process_file(&zip_path, 1);
        assert!(!zip_path.is_file());
        let extracted_dir = td.join("sample.zip");
        assert!(extracted_dir.exists());
        let inner = extracted_dir.join("inner.txt");
        assert_eq!(std::fs::read_to_string(inner).unwrap(), "hi");
        std::fs::remove_dir_all(&td).ok();
    }

    #[test]
    fn test_untar_or_gz_handles_tar() {
        let td = temp_dir();
        let tar_path = td.join("archive.tar");
        let file_in = td.join("f.txt");
        std::fs::write(&file_in, b"abc").unwrap();
        let tar_file = std::fs::File::create(&tar_path).unwrap();
        let mut builder = tar::Builder::new(tar_file);
        builder.append_path_with_name(&file_in, "f.txt").unwrap();
        builder.finish().unwrap();
        let res = untar_or_gz(&tar_path, 2);
        assert!(res.is_ok());
        let out_dir = td.join("archive.tar");
        assert_eq!(std::fs::read_to_string(out_dir.join("f.txt")).unwrap(), "abc");
        std::fs::remove_dir_all(&td).ok();
    }

    #[test]
    fn test_untar_or_gz_handles_tar_gz() {
        let td = temp_dir();
        let tar_gz_path = td.join("pkg.tar.gz");
        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(std::io::Cursor::new(&mut tar_buf));
            let data = b"hello tgz";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_cksum();
            header.set_mode(0o644);
            header.set_mtime(0);
            builder.append_data(&mut header, "a.txt", &data[..]).unwrap();
            builder.finish().unwrap();
        }
        {
            use flate2::write::GzEncoder;
            use flate2::Compression;
            let f = std::fs::File::create(&tar_gz_path).unwrap();
            let mut enc = GzEncoder::new(f, Compression::default());
            enc.write_all(&tar_buf).unwrap();
            enc.finish().unwrap();
        }
        let res = untar_or_gz(&tar_gz_path, 3);
        assert!(res.is_ok());
        let out_dir = td.join("pkg.tar.gz");
        assert_eq!(std::fs::read_to_string(out_dir.join("a.txt")).unwrap(), "hello tgz");
        std::fs::remove_dir_all(&td).ok();
    }

    #[test]
    fn test_untar_or_gz_handles_gz_single_file() {
        let td = temp_dir();
        let gz_path = td.join("doc.gz");
        let content = b"just gz";
        {
            use flate2::write::GzEncoder;
            use flate2::Compression;
            let f = std::fs::File::create(&gz_path).unwrap();
            let mut enc = GzEncoder::new(f, Compression::default());
            enc.write_all(content).unwrap();
            enc.finish().unwrap();
        }
        let res = untar_or_gz(&gz_path, 4);
        assert!(res.is_ok());
        let out_dir = td.join("doc.gz");
        let out_file = out_dir.join("doc");
        assert_eq!(std::fs::read_to_string(out_file).unwrap(), "just gz");
        std::fs::remove_dir_all(&td).ok();
    }

    #[test]
    fn test_process_file_ignores_temp_extensions() {
        let td = temp_dir();
        let tmp = td.join("ongoing.zip.part");
        std::fs::write(&tmp, b"x").unwrap();
        process_file(&tmp, 5);
        assert!(tmp.exists());
        std::fs::remove_dir_all(&td).ok();
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let dir_to_watch = args.watch_path.unwrap_or_else(|| platform::default_downloads_dir());

    println!("[Main] Target directory set to: {}", dir_to_watch.display());
    if !dir_to_watch.exists() {
        eprintln!("[Main] Error: Watch directory does not exist.");
        return Ok(());
    }


    let (tx, rx) = mpsc::channel::<PathBuf>();
    let rx = Arc::new(Mutex::new(rx));

    // Shutdown flag
    let shutting_down = Arc::new(AtomicBool::new(false));

    const NUM_WORKERS: usize = 4;
    for i in 0..NUM_WORKERS {
        let worker_rx = Arc::clone(&rx);
        let sd = Arc::clone(&shutting_down);
        thread::spawn(move || {
            println!("[Worker {}] Starting up.", i);
            loop {
                if sd.load(Ordering::SeqCst) {
                    println!("[Worker {}] Shutdown flag set. Exiting.", i);
                    break;
                }
                let path_result = worker_rx.lock().unwrap().recv();
                match path_result {
                    Ok(path) => process_file(&path, i),
                    Err(_) => {
                        println!("[Worker {}] Channel closed. Shutting down.", i);
                        break;
                    }
                }
            }
        });
    }

    println!("[Main] Checking for existing archives in {}...", dir_to_watch.display());
    for entry in WalkDir::new(&dir_to_watch).max_depth(1).into_iter().filter_map(std::result::Result::ok) {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
                match ext.as_str() {
                    "zip" | "rar" | "7z" | "tar" | "gz" => {
                        println!("[Main] Found existing archive: {}. Sending to worker.", path.display());
                        tx.send(path.to_path_buf()).expect("Failed to send path to worker thread");
                    }
                    _ => {}
                }
            }
        }
    }

    let watcher_tx = tx.clone();
    let sd_cb = Arc::clone(&shutting_down);
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if sd_cb.load(Ordering::SeqCst) {
            return; // ignore events during shutdown
        }
        match res {
            Ok(event) => match event.kind {
                EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Name(_)) => {
                    for path in event.paths {
                        if path.is_file() {
                            if let Some(ext) = path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
                                match ext.as_str() {
                                    "zip" | "rar" | "7z" | "tar" | "gz" => {
                                        println!("[Main] Detected file event for: {}. Sending to worker.", path.display());
                                        watcher_tx.send(path).expect("Failed to send path to worker thread");
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                _ => (),
            },
            Err(e) => eprintln!("[Main] Watch error: {:?}", e),
        }
    })?;

    println!("[Main] Watching directory: {} for new archives...", dir_to_watch.display());
    watcher.watch(&dir_to_watch, RecursiveMode::NonRecursive)?;

    // Install Ctrl+C handler to initiate graceful shutdown
    let sd_sig = Arc::clone(&shutting_down);
    ctrlc::set_handler(move || {
        if !sd_sig.swap(true, Ordering::SeqCst) {
            eprintln!("\n[Main] Ctrl+C received. Shutting down gracefully...");
        }
    }).expect("Error setting Ctrl+C handler");

    drop(tx);

    // Wait until shutdown flag is set
    while !shutting_down.load(Ordering::SeqCst) {
        thread::park_timeout(std::time::Duration::from_millis(200));
    }

    // Drop watcher to stop OS handles and callbacks
    drop(watcher);

    // Unpark workers so they can observe shutdown and exit
    // Dropping the channel sender already causes recv() to return Err
    // Give a brief moment for threads to unwind and release resources
    thread::sleep(std::time::Duration::from_millis(200));

    println!("[Main] Shutdown complete.");
    Ok(())
}