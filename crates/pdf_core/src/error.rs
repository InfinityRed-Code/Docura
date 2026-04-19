use thiserror::Error;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("document not loaded")]
    DocumentNotLoaded,
    #[error("page index {0} out of range (pages: {1})")]
    PageOutOfRange(u32, u32),
    #[error("engine error: {0}")]
    Engine(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
