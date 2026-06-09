//! Windows certificate-store signer (CNG). Implemented below; see `cng` impl.

use crate::error::SignResult;

mod cng;

pub use cng::{list_certificates, WindowsSigner, WinCertEntry};

/// Re-export for callers that only need the listing type.
pub fn list_signing_certificates() -> SignResult<Vec<WinCertEntry>> {
    list_certificates()
}
