//! Traits that isolate platform-dependent services from the core.
//!
//! Concrete implementations live in the Tauri app; the core only speaks
//! to the traits defined here.

use std::path::{Path, PathBuf};

use pdf_types::RecentFile;

pub type ServiceResult<T> = Result<T, ServiceError>;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("user cancelled")]
    Cancelled,
    #[error("io: {0}")]
    Io(String),
    #[error("platform: {0}")]
    Platform(String),
}

pub trait FileDialog: Send + Sync {
    fn pick_pdf(&self) -> ServiceResult<Option<PathBuf>>;
    fn pick_save_path(&self, suggested_name: &str) -> ServiceResult<Option<PathBuf>>;
}

pub trait FileSystem: Send + Sync {
    fn read(&self, path: &Path) -> ServiceResult<Vec<u8>>;
    fn write(&self, path: &Path, bytes: &[u8]) -> ServiceResult<()>;
    fn exists(&self, path: &Path) -> bool;
}

pub trait Opener: Send + Sync {
    fn reveal_in_file_manager(&self, path: &Path) -> ServiceResult<()>;
    fn open_with_default(&self, path: &Path) -> ServiceResult<()>;
}

pub trait Settings: Send + Sync {
    fn recent_files(&self) -> ServiceResult<Vec<RecentFile>>;
    fn push_recent(&self, entry: RecentFile) -> ServiceResult<()>;
}
