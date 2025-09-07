use clap::Parser;
use notify::{Watcher, RecursiveMode, Result, EventKind};
use std::path::{Path, PathBuf};
use std::fs;
use std::io;
use zip::ZipArchive;
use walkdir::WalkDir;
use std::thread;
use std::sync::{mpsc, Arc, Mutex};

mod platform;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short = 'p', long)]
    watch_path: Option<PathBuf>,
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
    if path.extension().map_or(false, |ext| ext != "zip") {
        return;
    }

    println!("[Worker {}] Processing new zip file: {}", worker_id, path.display());

    if let Err(e) = unzip_file(path, worker_id) {
        eprintln!("[Worker {}] Error unzipping {}: {}", worker_id, path.display(), e);
        return;
    }

    if let Err(e) = fs::remove_file(path) {
        eprintln!("[Worker {}] Error deleting {}: {}", worker_id, path.display(), e);
        return;
    }

    println!("[Worker {}] Successfully deleted original zip file: {}", worker_id, path.display());
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

    println!("[Main] Checking for existing zip files in {}...", dir_to_watch.display());
    for entry in WalkDir::new(&dir_to_watch).max_depth(1).into_iter().filter_map(std::result::Result::ok) {
        let path = entry.path();
        if path.is_file() && path.extension().map_or(false, |ext| ext == "zip") {
            println!("[Main] Found existing zip file: {}. Sending to worker.", path.display());
            tx.send(path.to_path_buf()).expect("Failed to send path to worker thread");
        }
    }

    let watcher_tx = tx.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
        Ok(event) => match event.kind {
            EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Name(_)) => {
                for path in event.paths {
                    if path.is_file() {
                        println!("[Main] Detected file event for: {}. Sending to worker.", path.display());
                        watcher_tx.send(path).expect("Failed to send path to worker thread");
                    }
                }
            }
            _ => (),
        },
        Err(e) => eprintln!("[Main] Watch error: {:?}", e),
    })?;

    println!("[Main] Watching directory: {} for new zip files...", dir_to_watch.display());
    watcher.watch(&dir_to_watch, RecursiveMode::NonRecursive)?;

    drop(tx);

    loop {
        thread::park();
    }
}