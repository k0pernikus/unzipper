use clap::Parser;

use fs2::FileExt;

use notify::{EventKind, RecursiveMode, Result, Watcher};

use std::fs;

use std::io;

use std::path::{Path, PathBuf};

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};

use std::thread;

use walkdir::WalkDir;

mod extractors;

mod platform;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]

struct Args {
    #[arg(short = 'p', long)]
    watch_path: Option<PathBuf>,
}

fn is_processable_archive_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "zip" | "rar" | "7z" | "tar" | "gz"
    )
}

fn is_processable_path(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .map_or(false, is_processable_archive_extension)
}

fn delete_file(path: &Path, worker_id: usize) {
    println!(
        "[Worker {}] Trying to delete file: {}",
        worker_id,
        path.display()
    );

    let _ = fs::remove_file(path);
}

pub(crate) fn wait_until_stable(
    path: &Path,

    attempts: usize,

    delay: std::time::Duration,
) -> io::Result<()> {
    let mut prev_len = None;

    for _ in 0..attempts {
        match fs::metadata(path) {
            Ok(m) => {
                let len = m.len();

                if let Some(p) = prev_len {
                    if p == len {
                        return Ok(());
                    }
                }

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

    let sanitized: String = file_name
        .chars()
        .filter(|c| !"<>:\"/\\|?*".contains(*c))
        .collect();

    parent_dir.join(sanitized)
}

pub(crate) fn prepare_dest_dir(path: &Path) -> io::Result<(PathBuf, Option<PathBuf>)> {
    let dest_dir = build_dest_dir_with_extension(path);

    if dest_dir.is_dir() {
        return Ok((dest_dir, None));
    }

    if dest_dir.exists() {
        let mut tmp_name = dest_dir
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| format!("{}.__extracting__", s))
            .unwrap_or_else(|| String::from("__extracting__"));

        let mut tmp_path = dest_dir
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(&tmp_name);

        let mut counter = 1;

        while tmp_path.exists() {
            tmp_name = format!(
                "{}.__extracting__{}",
                dest_dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("archive"),
                counter
            );

            tmp_path = dest_dir
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(&tmp_name);

            counter += 1;
        }

        fs::rename(path, &tmp_path)?;

        fs::create_dir_all(&dest_dir)?;

        return Ok((dest_dir, Some(tmp_path)));
    }

    fs::create_dir_all(&dest_dir)?;

    Ok((dest_dir, None))
}

fn is_temp_file_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();

    lower.ends_with(".crdownload") || lower.ends_with(".part") || lower.ends_with(".tmp")
}

fn process_file(path: &Path, worker_id: usize) {
    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
        if is_temp_file_name(name) {
            return;
        }
    }

    if let Err(e) = wait_until_stable(path, 5, std::time::Duration::from_millis(300)) {
        eprintln!(
            "[Worker {}] Skipping {} due to stability check error: {}",
            worker_id,
            path.display(),
            e
        );

        return;
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());

    let ext = match ext {
        Some(e) => e,

        None => return,
    };

    use crate::extractors::ArchiveExtractor;

    let extractor: Option<Box<dyn ArchiveExtractor>> = match ext.as_str() {
        "zip" => Some(Box::new(crate::extractors::zip::ZipExtractor)),

        "tar" | "tgz" | "gz" | "tar.gz" | "taz" => {
            Some(Box::new(crate::extractors::targz::TarGzExtractor))
        }

        "7z" => Some(Box::new(crate::extractors::sevenz::SevenZExtractor)),

        "rar" => Some(Box::new(crate::extractors::rar::RarExtractor)),

        _ => None,
    };

    let Some(extractor) = extractor else { return };

    println!(
        "[Worker {}] Processing {} file: {}",
        worker_id,
        ext,
        path.display()
    );

    let original_path = path.to_path_buf();

    let (_, renamed_path) = match prepare_dest_dir(&original_path) {
        Ok((dest_dir, renamed_path_opt)) => {
            if let Some(renamed) = &renamed_path_opt {
                println!(
                    "[Worker {}] Renamed file from {} to {}",
                    worker_id,
                    original_path.display(),
                    renamed.display()
                );
            }

            (dest_dir, renamed_path_opt)
        }

        Err(e) => {
            eprintln!(
                "[Worker {}] Failed to prepare destination directory for {}: {}",
                worker_id,
                original_path.display(),
                e
            );

            return;
        }
    };

    let file_to_extract = renamed_path.as_ref().unwrap_or(&original_path);

    if let Err(e) = extractor.extract(file_to_extract, worker_id) {
        eprintln!(
            "[Worker {}] Error extracting {}: {}",
            worker_id,
            file_to_extract.display(),
            e
        );

        return;
    }

    if let Err(e) = wait_until_stable(file_to_extract, 5, std::time::Duration::from_millis(300)) {
        eprintln!(
            "[Worker {}] Failed to achieve stability on {}: {}",
            worker_id,
            file_to_extract.display(),
            e
        );

        return;
    }

    delete_file(file_to_extract, worker_id);
}

#[cfg(test)]

mod tests {
    use super::*;

    use std::io::Write;

    fn temp_dir() -> PathBuf {
        let mut d = std::env::temp_dir();

        d.push(format!(
            "unzipper_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

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

        p.push(format!(
            "unzipper_test_{}_{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

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

        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

        z.start_file("inner.txt", options).unwrap();

        z.write_all(b"hi").unwrap();

        z.finish().unwrap();

        zip_path
    }

    #[test]

    fn test_unzip_file_extracts() {
        use crate::extractors::{zip::ZipExtractor, ArchiveExtractor};

        let td = temp_dir();

        let zip_path = create_sample_zip(&td);

        let res = ZipExtractor.extract(&zip_path, 0);

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
    println!("### UNIQUE VERSION: 2025-09-08T02:05:00Z ###");

    let args = Args::parse();

    let lock_file_path = std::env::temp_dir().join("unzipper.lock");

    let lock_file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(&lock_file_path)
        .expect("Could not create lock file.");

    if let Err(_e) = lock_file.try_lock_exclusive() {
        eprintln!("Another instance of unzipper is already running. Exiting.");

        return Ok(());
    }

    let dir_to_watch = args
        .watch_path
        .unwrap_or_else(|| platform::default_downloads_dir());

    println!("[Main] Target directory set to: {}", dir_to_watch.display());

    if !dir_to_watch.exists() {
        eprintln!("[Main] Error: Watch directory does not exist.");

        return Ok(());
    }

    let (tx_to_workers, rx_from_main) = mpsc::channel::<PathBuf>();

    let rx_from_main = Arc::new(Mutex::new(rx_from_main));

    let (tx_removals, rx_removals) = mpsc::channel::<PathBuf>();

    let shutting_down = Arc::new(AtomicBool::new(false));

    let sd_cb_removals = Arc::clone(&shutting_down);

    let sd_cb_removals_for_thread = Arc::clone(&shutting_down);

    let watcher_tx_removals = tx_removals.clone();

    let dir_to_watch_removals = dir_to_watch.clone();

    thread::spawn(move || {
        println!("[REMOVAL_CHECK] Starting up.");

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if sd_cb_removals.load(Ordering::SeqCst) {
                return;
            }

            if let Ok(event) = res {
                if event.kind == EventKind::Remove(notify::event::RemoveKind::Any) {
                    for path in event.paths {
                        if is_processable_path(path.as_path()) {
                            watcher_tx_removals
                                .send(path)
                                .expect("Failed to send removal event to main thread");
                        }
                    }
                }
            }
        })
        .expect("Failed to create removal watcher");

        watcher
            .watch(&dir_to_watch_removals, RecursiveMode::NonRecursive)
            .expect("Failed to start removal watcher");

        while !sd_cb_removals_for_thread.load(Ordering::SeqCst) {
            thread::park_timeout(std::time::Duration::from_millis(200));
        }

        drop(watcher);
    });

    const NUM_WORKERS: usize = 4;

    for i in 0..NUM_WORKERS {
        let worker_rx = Arc::clone(&rx_from_main);

        let sd = Arc::clone(&shutting_down);

        let tx_removals_worker = tx_removals.clone();

        thread::spawn(move || {
            println!("[Worker {}] Starting up.", i);

            loop {
                if sd.load(Ordering::SeqCst) {
                    println!("[Worker {}] Shutdown flag set. Exiting.", i);

                    break;
                }

                let path_result = worker_rx.lock().unwrap().recv();

                match path_result {
                    Ok(path) => {
                        process_file(&path, i);

                        tx_removals_worker
                            .send(path)
                            .expect("Failed to send delete signal to main thread");
                    }

                    Err(_) => {
                        println!("[Worker {}] Channel closed. Shutting down.", i);

                        break;
                    }
                }
            }
        });
    }

    println!(
        "[Main] Checking for existing archives in {}...",
        dir_to_watch.display()
    );

    for entry in WalkDir::new(&dir_to_watch)
        .max_depth(1)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();

        if is_processable_path(path) {
            println!(
                "[Main] Found existing archive: {}. Sending to worker.",
                path.display()
            );

            tx_to_workers
                .send(path.to_path_buf())
                .expect("Failed to send path to worker thread");
        }
    }

    println!("[Main] Finished scanning for existing archives.");

    let watcher_tx_to_workers = tx_to_workers.clone();

    let sd_cb_main_watcher = Arc::clone(&shutting_down);

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if sd_cb_main_watcher.load(Ordering::SeqCst) {
            return;
        }

        match res {
            Ok(event) => match event.kind {
                EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Name(_)) => {
                    for path in event.paths {
                        if is_processable_path(&path) {
                            println!(
                                "[Main] Detected file event for: {}. Sending to worker.",
                                path.display()
                            );

                            watcher_tx_to_workers
                                .send(path)
                                .expect("Failed to send path to worker thread");
                        }
                    }
                }

                _ => (),
            },

            Err(e) => eprintln!("[Main] Watch error: {:?}", e),
        }
    })?;

    println!(
        "[Main] Watching directory: {} for new archives...",
        dir_to_watch.display()
    );

    watcher.watch(&dir_to_watch, RecursiveMode::NonRecursive)?;

    let sd_sig = Arc::clone(&shutting_down);

    ctrlc::set_handler(move || {
        if !sd_sig.swap(true, Ordering::SeqCst) {
            eprintln!("\n[Main] Ctrl+C received. Shutting down gracefully...");
        }
    })
    .expect("Error setting Ctrl+C handler");

    drop(tx_to_workers);

    drop(tx_removals);

    while !shutting_down.load(Ordering::SeqCst) {
        if let Ok(removed_path) = rx_removals.try_recv() {
            println!(
                "[Main] Confirmed file deletion via event: {}",
                removed_path.display()
            );
        }

        thread::park_timeout(std::time::Duration::from_millis(200));
    }

    drop(watcher);

    thread::sleep(std::time::Duration::from_millis(200));

    println!("[Main] Shutdown complete.");

    Ok(())
}
