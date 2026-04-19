//! UI- and OS-agnostic domain logic for the PDF viewer.
//!
//! No Tauri, no filesystem paths as business logic, no frontend types.

pub mod engine;
pub mod error;
pub mod viewer;

pub use engine::{PdfEngine, RenderRequest, RenderedPage};
pub use error::{CoreError, CoreResult};
pub use viewer::ViewerState;
