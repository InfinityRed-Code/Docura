//! Serialisation with PDF/A file-structure rules.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use lopdf::xref::XrefType;
use lopdf::{Object, SaveOptions, StringFormat};

use crate::util;
use crate::{Ctx, PdfaError, Result};

/// 16 pseudo-random bytes derived from `seed` and the clock.
pub fn random_id(seed: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for round in 0..2u8 {
        let mut h = DefaultHasher::new();
        seed.hash(&mut h);
        t.hash(&mut h);
        round.hash(&mut h);
        std::process::id().hash(&mut h);
        out.extend_from_slice(&h.finish().to_be_bytes());
    }
    out
}

pub fn save(ctx: &mut Ctx) -> Result<Vec<u8>> {
    let level = ctx.level;
    let doc = &mut ctx.doc;

    // Trailer: /ID (keep the permanent part), no /Encrypt.
    let first = match doc.trailer.get(b"ID") {
        Ok(Object::Array(a)) => match a.first() {
            Some(Object::String(s, _)) if !s.is_empty() => Some(s.clone()),
            _ => None,
        },
        _ => None,
    };
    let seed = ctx.source.get(..ctx.source.len().min(4096)).unwrap_or(&[]);
    let first = first.unwrap_or_else(|| random_id(seed));
    let second = random_id(&first);
    doc.trailer.set(
        "ID",
        Object::Array(vec![Object::String(first, StringFormat::Hexadecimal), Object::String(second, StringFormat::Hexadecimal)]),
    );
    doc.trailer.remove(b"Encrypt");
    for k in [b"Prev".as_slice(), b"XRefStm", b"Type", b"W", b"Index", b"Filter", b"DecodeParms", b"Length"] {
        doc.trailer.remove(k);
    }

    util::prune_unreachable(doc);
    doc.renumber_objects();

    // Every stream gets a direct, correct /Length.
    for obj in doc.objects.values_mut() {
        if let Object::Stream(s) = obj {
            let len = s.content.len() as i64;
            s.dict.set("Length", len);
            s.allows_compression = false;
        }
    }

    doc.version = level.pdf_version().to_string();
    doc.binary_mark = vec![0xE2, 0xE3, 0xCF, 0xD3];
    let mut out = Vec::new();
    if level.allows_object_streams() {
        // One object stream for everything: lopdf 0.38 only lists the first
        // object stream in the cross-reference stream, so a second one
        // (more than 100 objects by default) would leave its objects
        // unreachable for strict readers.
        let opts = SaveOptions::builder().use_object_streams(true).use_xref_streams(true).max_objects_per_stream(usize::MAX).compression_level(6).build();
        doc.save_with_options(&mut out, opts).map_err(|e| PdfaError::Write(e.to_string()))?;
        // lopdf bumps the header for object streams; restore the level's.
        fix_header(&mut out, level.pdf_version());
    } else {
        doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
        doc.save_to(&mut out).map_err(|e| PdfaError::Write(e.to_string()))?;
    }
    out.push(b'\n');
    Ok(out)
}

fn fix_header(out: &mut [u8], version: &str) {
    let want = format!("%PDF-{version}");
    if out.len() >= want.len() && out.starts_with(b"%PDF-") {
        out[..want.len()].copy_from_slice(want.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_differ() {
        let a = random_id(b"x");
        let b = random_id(&a);
        assert_eq!(a.len(), 16);
        assert_ne!(a, b);
        let mut h = b"%PDF-1.5\n".to_vec();
        fix_header(&mut h, "1.7");
        assert_eq!(&h, b"%PDF-1.7\n");
    }
}
