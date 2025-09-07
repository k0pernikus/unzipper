use std::io;
use std::path::Path;

pub trait ArchiveExtractor {
    fn extract(&self, path: &Path, worker_id: usize) -> io::Result<()>;
}

pub mod zip;
pub mod targz;
pub mod sevenz;
pub mod rar;
