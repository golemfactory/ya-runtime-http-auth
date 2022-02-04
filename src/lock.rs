use fs2::FileExt;
use std::fs::File;
use std::path::{Path, PathBuf};

pub const LOCK_FILE_EXT: &str = ".lock";

pub fn with_lock_ext<P: AsRef<Path>>(path: P) -> PathBuf {
    let mut path = path.as_ref().to_owned();
    path.push(LOCK_FILE_EXT);
    path
}

pub struct LockFile {
    path: PathBuf,
    file: Option<File>,
}

impl LockFile {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            file: None,
        }
    }

    pub fn is_locked(&self) -> bool {
        self.file.is_some()
    }

    pub fn lock(&mut self) -> std::io::Result<()> {
        let file = if self.path.is_file() {
            File::open(&self.path)?
        } else {
            File::create(&self.path)?
        };

        file.try_lock_exclusive()?;
        self.file.replace(file);

        Ok(())
    }

    pub fn unlock(&mut self) -> std::io::Result<()> {
        if let Some(f) = self.file.take() {
            f.unlock()?;
            std::fs::remove_file(&self.path)?;
        }

        Ok(())
    }
}

impl Drop for LockFile {
    fn drop(&mut self) {
        let _ = self.unlock();
    }
}
