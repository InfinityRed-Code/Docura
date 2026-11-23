//! Logical structure for level a (filled in by the tagging milestone).

use crate::{Ctx, PdfaError, Result};

pub fn run(ctx: &mut Ctx) -> Result<()> {
    if ctx.level.needs_structure() {
        return Err(PdfaError::Unsupported("level a is not available yet".into()));
    }
    Ok(())
}
