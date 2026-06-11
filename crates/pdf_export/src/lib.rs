//! Document export: PDF text → Word (.docx) and Excel (.xlsx).
//!
//! Input is the engine's per-page `TextRun` layout; `flow` reconstructs
//! lines/paragraphs/headings, `docx` and `xlsx` serialise them. Layout
//! fidelity is a non-goal — the output is meant for further editing.

pub mod flow;

mod docx;
mod xlsx;

pub use docx::build_docx;
pub use flow::{reconstruct_blocks, Block};
pub use xlsx::build_xlsx;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("build: {0}")]
    Build(String),
}
