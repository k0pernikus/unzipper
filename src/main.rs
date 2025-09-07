use clap::Parser;
use notify::{Watcher, RecursiveMode, Result, EventKind};
use std::path::{Path, PathBuf};
use std::fs;
use std::io;
use zip::ZipArchive;
use walkdir::WalkDir;
use std::thread;
use std::sync::{mpsc, Arc, Mutex};
use std::process::Command;
use flate2::read::GzDecoder;
use tar::Archive as TarArchive;

mod platform;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short = 'p', long)]
    watch_path: Option<PathBuf>,
}

fn wait_until_stable(path: &Path, attempts: usize, delay: std::time::Duration) -> io::Result<()> {
    let mut prev_len = None;
    for _ in 0..attempts {
        match fs::metadata(path) {
            Ok(m) => {
                let len = m.len();
                if let Some(p) = prev_len { if p == len { return Ok(()); } }
                prev_len = Some(len);
            }
            Err(e) => {
                // If file not found or access denied, wait and retry
                let _ = e.kind();
            }
        }
        std::thread::sleep(delay);
    }
    Ok(())
}

fn unzip_file(zip_path: &Path, worker_id: usize) -> io::Result<()> {
    let parent_dir = zip_path.parent().unwrap_or_else(|| Path::new(""));
    let file_stem = zip_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let sanitized_stem: String = file_stem.chars().filter(|c| !"<>:\"/\\|?*".contains(*c)).collect();
    let dest_dir = parent_dir.join(sanitized_stem);

    println!("[Worker {}] Unzipping file: {} to {}", worker_id, zip_path.display(), dest_dir.display());

    fs::create_dir_all(&dest_dir)?;

    let file = fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        println!("[Worker {}] Extracting: {}", worker_id, file.name());

        let outpath = match file.enclosed_name() {
            Some(path) => dest_dir.join(path),
            None => continue,
        };

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
            continue;
        }

        if let Some(p) = outpath.parent() {
            if !p.exists() {
                fs::create_dir_all(p)?;
            }
        }

        let mut outfile = fs::File::create(&outpath)?;
        io::copy(&mut file, &mut outfile)?;
    }

    println!("[Worker {}] Successfully unzipped {}", worker_id, zip_path.display());
    Ok(())
}

fn process_file(path: &Path, worker_id: usize) {
    // Ignore temporary/incomplete download extensions
    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".crdownload") || lower.ends_with(".part") || lower.ends_with(".tmp") {
            return;
        }
    }
    // Wait for the file to become stable before processing (helps on Windows when the browser is still writing)
    if let Err(e) = wait_until_stable(path, 5, std::time::Duration::from_millis(300)) {
            eprintln!("[Worker {}] Skipping {} due to stability check error: {}", worker_id, path.display(), e);
            return;
    }
    let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());
    let ext = match ext { Some(e) => e, None => return };

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

    if let Err(e) = fs::remove_file(path) {
        eprintln!("[Worker {}] Error deleting {}: {}", worker_id, path.display(), e);
        return;
    }

    println!("[Worker {}] Successfully deleted original archive: {}", worker_id, path.display());
}

fn untar_or_gz(path: &Path, worker_id: usize) -> io::Result<()> {
    let parent_dir = path.parent().unwrap_or_else(|| Path::new(""));
    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let mut name = file_stem.to_string();
    if path.extension().and_then(|e| e.to_str()) == Some("gz") {
        if name.ends_with(".tar") { name = name.trim_end_matches(".tar").to_string(); }
    }
    let sanitized: String = name.chars().filter(|c| !"<>:\"/\\|?*".contains(*c)).collect();
    let dest_dir = parent_dir.join(sanitized);
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
        let out_file_path = dest_dir.join(file_stem);
        let mut out = fs::File::create(out_file_path)?;
        io::copy(&mut gz, &mut out)?;
    }

    println!("[Worker {}] Extracted tar/gz {}", worker_id, path.display());
    Ok(())
}

fn extract_7z(path: &Path, worker_id: usize) -> io::Result<()> {
    let parent_dir = path.parent().unwrap_or_else(|| Path::new(""));
    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let sanitized: String = file_stem.chars().filter(|c| !"<>:\"/\\|?*".contains(*c)).collect();
    let dest_dir = parent_dir.join(sanitized);
    fs::create_dir_all(&dest_dir)?;

    // On Windows, newly downloaded files may still be locked. Wait briefly for stability.
    wait_until_stable(path, 5, std::time::Duration::from_millis(300))?;

    let mut sz = sevenz_rust::SevenZReader::open(path, sevenz_rust::Password::empty())
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    sz.for_each_entries(|entry, mut reader| {
            let name = entry.name();
            let out = dest_dir.join(name);
            // Ensure the output path stays within dest_dir
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

    println!("[Worker {}] Extracted 7z {}", worker_id, path.display());
    Ok(())
}

fn extract_rar(path: &Path, worker_id: usize) -> io::Result<()> {
    let parent_dir = path.parent().unwrap_or_else(|| Path::new(""));
    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let sanitized: String = file_stem.chars().filter(|c| !"<>:\"/\\|?*".contains(*c)).collect();
    let dest_dir = parent_dir.join(sanitized);
    fs::create_dir_all(&dest_dir)?;

    let output = Command::new("7z")
        .arg("x")
        .arg("-y")
        .arg(format!("-o{}", dest_dir.display()))
        .arg(path)
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

fn main() -> Result<()> {
    let args = Args::parse();

    let dir_to_watch = args.watch_path.unwrap_or_else(|| platform::default_downloads_dir());

    println!("[Main] Target directory set to: {}", dir_to_watch.display());
    if !dir_to_watch.exists() {
        eprintln!("[Main] Error: Watch directory does not exist.");
        return Ok(())
    }


    let (tx, rx) = mpsc::channel::<PathBuf>();
    let rx = Arc::new(Mutex::new(rx));

    const NUM_WORKERS: usize = 4;
    for i in 0..NUM_WORKERS {
        let worker_rx = Arc::clone(&rx);
        thread::spawn(move || {
            println!("[Worker {}] Starting up.", i);
            loop {
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
                match ext.as_str() { "zip" | "rar" | "7z" | "tar" | "gz" => {
                    println!("[Main] Found existing archive: {}. Sending to worker.", path.display());
                    tx.send(path.to_path_buf()).expect("Failed to send path to worker thread");
                }
                _ => {}
                }
            }
        }
    }

    let watcher_tx = tx.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
        Ok(event) => match event.kind {
            EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Name(_)) => {
                for path in event.paths {
                    if path.is_file() {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
                            match ext.as_str() { "zip" | "rar" | "7z" | "tar" | "gz" => {
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
    })?;

    println!("[Main] Watching directory: {} for new archives...", dir_to_watch.display());
    watcher.watch(&dir_to_watch, RecursiveMode::NonRecursive)?;

    drop(tx);

    loop {
        thread::park();
    }
}