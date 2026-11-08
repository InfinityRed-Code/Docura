//! PDF-level signature scaffolding.
//!
//! Adds a signature field + signature dictionary to a PDF as an **incremental
//! update** (the original bytes are preserved verbatim, so any pre-existing
//! signature stays valid), serializes it with a fixed-size `/Contents`
//! placeholder, computes the `/ByteRange`, and patches the placeholders in
//! place. The actual CMS blob is written later via [`embed`].

use lopdf::{Dictionary, Document, IncrementalDocument, Object, StringFormat};

use crate::error::{SignError, SignResult};

/// Metadata written into the signature dictionary.
#[derive(Clone, Debug, Default)]
pub struct SigDictMeta {
    /// PDF date string for `/M`, e.g. `D:20260609120000+00'00'` (no parens).
    pub pdf_date: String,
    pub reason: Option<String>,
    pub name: Option<String>,
    pub location: Option<String>,
    pub contact: Option<String>,
    /// Bytes reserved for the hex `/Contents` placeholder (the CMS blob).
    pub placeholder_len: usize,
    /// Unique-ish suffix for the field name `/T` (e.g. epoch seconds).
    pub field_seq: i64,
}

/// Result of [`prepare`]: the serialized PDF with placeholders, the byte
/// offsets of the hex `/Contents` region, and the computed `/ByteRange`.
pub struct Prepared {
    pub bytes: Vec<u8>,
    /// `[start, end)` of the hex digits between the `<` and `>` of `/Contents`.
    pub gap: (usize, usize),
    /// `[off0, len0, off1, len1]` — the two signed ranges.
    pub byte_range: [usize; 4],
}

const BYTERANGE_PH: i64 = 2_000_000_000; // 10-digit width reservation

fn pdf_err<E: std::fmt::Display>(e: E) -> SignError {
    SignError::Pdf(e.to_string())
}

fn lit(s: &str) -> Object {
    Object::String(s.as_bytes().to_vec(), StringFormat::Literal)
}

/// A PDF *text string* (§7.9.2.2): ASCII as-is, anything else as UTF-16BE
/// with BOM. Raw UTF-8 bytes would be read as PDFDocEncoding and show up as
/// mojibake (e.g. "MÃ¼nchen") in viewers' signature panels.
fn text(s: &str) -> Object {
    if s.is_ascii() {
        return lit(s);
    }
    let mut bytes = vec![0xFE, 0xFF];
    for u in s.encode_utf16() {
        bytes.extend_from_slice(&u.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
}

/// Deref an `/Annots`- or `/Fields`-style entry from the previous document into
/// an owned `Vec<Object>` (handles inline arrays and array references).
fn read_array(prev: &Document, entry: Option<&Object>) -> Vec<Object> {
    match entry {
        Some(Object::Array(a)) => a.clone(),
        Some(Object::Reference(id)) => prev
            .get_object(*id)
            .ok()
            .and_then(|o| o.as_array().ok())
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Build the incremental update carrying the signature field + dictionary and
/// return the serialized bytes with placeholders patched.
pub fn prepare(pdf: &[u8], meta: &SigDictMeta) -> SignResult<Prepared> {
    let prev = Document::load_mem(pdf).map_err(pdf_err)?;

    let catalog_id = prev
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(pdf_err)?;

    // Target the first page for the (invisible) widget's /P.
    let page_id = prev
        .get_pages()
        .into_values()
        .next()
        .ok_or_else(|| SignError::Pdf("document has no pages".into()))?;

    // Snapshot existing AcroForm/Fields/Annots before mutating.
    let catalog = prev.get_object(catalog_id).and_then(Object::as_dict).map_err(pdf_err)?;
    let acroform_ref = catalog.get(b"AcroForm").and_then(Object::as_reference).ok();
    // An inline AcroForm dictionary carries the existing /Fields (and /DR,
    // /DA …) too; it must be extended, not replaced by a fresh one.
    let inline_acroform: Option<Dictionary> = catalog
        .get(b"AcroForm")
        .and_then(Object::as_dict)
        .ok()
        .cloned();
    let existing_fields: Vec<Object> = match (acroform_ref, &inline_acroform) {
        (Some(id), _) => {
            let af = prev.get_object(id).and_then(Object::as_dict).map_err(pdf_err)?;
            read_array(&prev, af.get(b"Fields").ok())
        }
        (None, Some(af)) => read_array(&prev, af.get(b"Fields").ok()),
        (None, None) => Vec::new(),
    };
    let page_dict = prev.get_object(page_id).and_then(Object::as_dict).map_err(pdf_err)?;
    let mut annots = read_array(&prev, page_dict.get(b"Annots").ok());

    let mut idoc = IncrementalDocument::create_from(pdf.to_vec(), prev);
    let nd = &mut idoc.new_document;

    // 1. Signature dictionary (/V).
    let mut sig = Dictionary::new();
    sig.set("Type", Object::Name(b"Sig".to_vec()));
    sig.set("Filter", Object::Name(b"Adobe.PPKLite".to_vec()));
    sig.set("SubFilter", Object::Name(b"ETSI.CAdES.detached".to_vec())); // PAdES
    sig.set(
        "ByteRange",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(BYTERANGE_PH),
            Object::Integer(BYTERANGE_PH),
            Object::Integer(BYTERANGE_PH),
        ]),
    );
    sig.set(
        "Contents",
        Object::String(vec![0u8; meta.placeholder_len], StringFormat::Hexadecimal),
    );
    if !meta.pdf_date.is_empty() {
        sig.set("M", lit(&format!("D:{}", meta.pdf_date.trim_start_matches("D:"))));
    }
    if let Some(r) = &meta.reason { sig.set("Reason", text(r)); }
    if let Some(n) = &meta.name { sig.set("Name", text(n)); }
    if let Some(l) = &meta.location { sig.set("Location", text(l)); }
    if let Some(c) = &meta.contact { sig.set("ContactInfo", text(c)); }
    let sig_id = nd.add_object(Object::Dictionary(sig));

    // 2. Signature field = widget annotation.
    let mut field = Dictionary::new();
    field.set("Type", Object::Name(b"Annot".to_vec()));
    field.set("Subtype", Object::Name(b"Widget".to_vec()));
    field.set("FT", Object::Name(b"Sig".to_vec()));
    field.set("T", lit(&format!("Signature_{}", meta.field_seq)));
    field.set("V", Object::Reference(sig_id));
    field.set("P", Object::Reference(page_id));
    field.set("Rect", Object::Array(vec![
        Object::Integer(0), Object::Integer(0), Object::Integer(0), Object::Integer(0),
    ]));
    field.set("F", Object::Integer(132)); // Print + Locked, zero-size = invisible
    let field_id = nd.add_object(Object::Dictionary(field));

    // 3. AcroForm: append field, set SigFlags=3 (SignaturesExist | AppendOnly).
    let mut new_fields = existing_fields;
    new_fields.push(Object::Reference(field_id));
    let acroform_id = match acroform_ref {
        Some(id) => {
            idoc.opt_clone_object_to_new_document(id).map_err(pdf_err)?;
            let af = idoc.new_document.get_object_mut(id).and_then(Object::as_dict_mut).map_err(pdf_err)?;
            af.set("Fields", Object::Array(new_fields));
            af.set("SigFlags", Object::Integer(3));
            id
        }
        None => {
            let mut af = inline_acroform.unwrap_or_default();
            af.set("Fields", Object::Array(new_fields));
            af.set("SigFlags", Object::Integer(3));
            idoc.new_document.add_object(Object::Dictionary(af))
        }
    };

    // 4. Catalog: point /AcroForm at the (possibly new) form.
    idoc.opt_clone_object_to_new_document(catalog_id).map_err(pdf_err)?;
    {
        let cat = idoc.new_document.get_object_mut(catalog_id).and_then(Object::as_dict_mut).map_err(pdf_err)?;
        cat.set("AcroForm", Object::Reference(acroform_id));
    }

    // 5. Page: append the widget to /Annots.
    annots.push(Object::Reference(field_id));
    idoc.opt_clone_object_to_new_document(page_id).map_err(pdf_err)?;
    {
        let pg = idoc.new_document.get_object_mut(page_id).and_then(Object::as_dict_mut).map_err(pdf_err)?;
        pg.set("Annots", Object::Array(annots));
    }

    // Serialize the incremental update.
    let mut bytes: Vec<u8> = Vec::new();
    idoc.save_to(&mut bytes).map_err(pdf_err)?;

    // Search only the appended update: the original bytes may hold earlier
    // signatures whose /ByteRange (or a zero-filled /Contents) would match
    // first — patching that one would invalidate the existing signature.
    let base = pdf.len();
    let gap = find_contents_gap(&bytes[base..], meta.placeholder_len)
        .map(|(s, e)| (s + base, e + base))
        .ok_or(SignError::PlaceholderNotFound)?;
    let total = bytes.len();
    let byte_range = [0usize, gap.0, gap.1, total - gap.1];
    patch_byte_range(&mut bytes[base..], byte_range)?;

    Ok(Prepared { bytes, gap, byte_range })
}

/// Locate the hex `/Contents` placeholder: a run of exactly `2*placeholder_len`
/// `'0'` bytes wrapped in `<` … `>`. Returns `[start, end)` of the hex digits.
fn find_contents_gap(bytes: &[u8], placeholder_len: usize) -> Option<(usize, usize)> {
    let run = placeholder_len * 2;
    if run == 0 { return None; }
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let start = i + 1;
            let end = start + run;
            if end < bytes.len()
                && bytes[end] == b'>'
                && bytes[start..end].iter().all(|&b| b == b'0')
            {
                return Some((start, end));
            }
        }
        i += 1;
    }
    None
}

/// Overwrite the `/ByteRange [0 PH PH PH]` placeholder with the real offsets,
/// padded with spaces so the total byte length (and thus all later offsets)
/// stays identical.
fn patch_byte_range(bytes: &mut [u8], br: [usize; 4]) -> SignResult<()> {
    let key = b"/ByteRange";
    let key_pos = find(bytes, key).ok_or(SignError::PlaceholderNotFound)?;
    let open = key_pos + (key_pos..bytes.len())
        .find(|&i| bytes[i] == b'[')
        .map(|i| i - key_pos)
        .ok_or(SignError::PlaceholderNotFound)?;
    let close = (open..bytes.len())
        .find(|&i| bytes[i] == b']')
        .ok_or(SignError::PlaceholderNotFound)?;
    let span = close - open + 1; // inclusive of [ and ]
    let real = format!("[0 {} {} {}]", br[1], br[2], br[3]);
    if real.len() > span {
        return Err(SignError::PlaceholderTooSmall { need: real.len(), have: span });
    }
    let mut out = real.into_bytes();
    // Pad before the closing ']' with spaces to preserve length.
    let pad = span - out.len();
    out.pop(); // remove ']'
    out.resize(out.len() + pad, b' ');
    out.push(b']');
    bytes[open..=close].copy_from_slice(&out);
    Ok(())
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Concatenate the two signed byte ranges (everything except the hex gap).
pub fn signed_message(bytes: &[u8], br: [usize; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(br[1] + br[3]);
    out.extend_from_slice(&bytes[br[0]..br[0] + br[1]]);
    out.extend_from_slice(&bytes[br[2]..br[2] + br[3]]);
    out
}

/// Write the DER CMS blob as uppercase hex into the `/Contents` gap. Remaining
/// gap bytes stay `'0'` (trailing zeros, ignored by DER parsers).
pub fn embed(bytes: &mut [u8], gap: (usize, usize), der: &[u8]) -> SignResult<()> {
    let cap = gap.1 - gap.0;
    let need = der.len() * 2;
    if need > cap {
        return Err(SignError::PlaceholderTooSmall { need, have: cap });
    }
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for (j, &b) in der.iter().enumerate() {
        bytes[gap.0 + j * 2] = HEX[(b >> 4) as usize];
        bytes[gap.0 + j * 2 + 1] = HEX[(b & 0xf) as usize];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    // Minimal one-page PDF.
    fn tiny_pdf() -> Vec<u8> {
        let mut d = Document::with_version("1.5");
        let pages_id = d.new_object_id();
        let page_id = d.new_object_id();
        let cat_id = d.new_object_id();
        d.objects.insert(page_id, Object::Dictionary({
            let mut p = Dictionary::new();
            p.set("Type", Object::Name(b"Page".to_vec()));
            p.set("Parent", Object::Reference(pages_id));
            p.set("MediaBox", Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]));
            p
        }));
        d.objects.insert(pages_id, Object::Dictionary({
            let mut p = Dictionary::new();
            p.set("Type", Object::Name(b"Pages".to_vec()));
            p.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
            p.set("Count", Object::Integer(1));
            p
        }));
        d.objects.insert(cat_id, Object::Dictionary({
            let mut c = Dictionary::new();
            c.set("Type", Object::Name(b"Catalog".to_vec()));
            c.set("Pages", Object::Reference(pages_id));
            c
        }));
        d.trailer.set("Root", Object::Reference(cat_id));
        d.max_id = cat_id.0;
        let mut out = Vec::new();
        d.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn prepare_preserves_prefix_and_computes_byterange() {
        let pdf = tiny_pdf();
        let meta = SigDictMeta { placeholder_len: 4096, field_seq: 42, ..Default::default() };
        let prepared = prepare(&pdf, &meta).unwrap();

        // Incremental update: original bytes are an exact prefix.
        assert!(prepared.bytes.len() > pdf.len());
        assert_eq!(&prepared.bytes[..pdf.len()], &pdf[..]);

        // Gap is the right size and all zeros.
        let (g0, g1) = prepared.gap;
        assert_eq!(g1 - g0, 4096 * 2);
        assert!(prepared.bytes[g0..g1].iter().all(|&b| b == b'0'));
        assert_eq!(prepared.bytes[g0 - 1], b'<');
        assert_eq!(prepared.bytes[g1], b'>');

        // ByteRange covers everything but the gap.
        let br = prepared.byte_range;
        assert_eq!(br[0], 0);
        assert_eq!(br[1], g0);
        assert_eq!(br[2], g1);
        assert_eq!(br[3], prepared.bytes.len() - g1);
        assert_eq!(signed_message(&prepared.bytes, br).len(), prepared.bytes.len() - (g1 - g0));

        // The patched /ByteRange must parse back to the same numbers.
        let s = String::from_utf8_lossy(&prepared.bytes);
        let needle = format!("[0 {} {} {}", br[1], br[2], br[3]);
        assert!(s.contains(&needle), "byterange not patched: looking for {needle}");
        // No leftover placeholder value.
        assert!(!s.contains(&BYTERANGE_PH.to_string()), "byterange placeholder not replaced");
    }

    #[test]
    fn embed_writes_hex_and_rejects_overflow() {
        let pdf = tiny_pdf();
        let meta = SigDictMeta { placeholder_len: 8, field_seq: 1, ..Default::default() };
        let mut prepared = prepare(&pdf, &meta).unwrap();
        let der = [0xDEu8, 0xAD, 0xBE, 0xEF];
        embed(&mut prepared.bytes, prepared.gap, &der).unwrap();
        let g0 = prepared.gap.0;
        assert_eq!(&prepared.bytes[g0..g0 + 8], b"DEADBEEF");
        // Rest of the gap stays zero.
        assert!(prepared.bytes[g0 + 8..prepared.gap.1].iter().all(|&b| b == b'0'));
        // Overflow rejected.
        let too_big = vec![0u8; 9]; // 18 hex > 16 cap
        assert!(embed(&mut prepared.bytes, prepared.gap, &too_big).is_err());
    }

    #[test]
    fn second_signature_leaves_the_first_untouched() {
        let meta = SigDictMeta { placeholder_len: 64, field_seq: 1, ..Default::default() };
        let mut first = prepare(&tiny_pdf(), &meta).unwrap();
        embed(&mut first.bytes, first.gap, &[0x30, 0x03, 0x02, 0x01, 0x01]).unwrap();

        let meta2 = SigDictMeta { field_seq: 2, ..meta };
        let second = prepare(&first.bytes, &meta2).unwrap();
        // The earlier revision (incl. its /ByteRange) is byte-identical…
        assert_eq!(&second.bytes[..first.bytes.len()], &first.bytes[..]);
        // …and the new placeholders were found/patched in the new update.
        assert!(second.gap.0 > first.bytes.len());
        let tail = String::from_utf8_lossy(&second.bytes[first.bytes.len()..]);
        let br = second.byte_range;
        assert!(tail.contains(&format!("[0 {} {} {}", br[1], br[2], br[3])), "new ByteRange not patched");
        assert!(!tail.contains(&BYTERANGE_PH.to_string()));
    }

    #[test]
    fn inline_acroform_fields_are_kept() {
        let mut d = Document::load_mem(&tiny_pdf()).unwrap();
        let field = d.add_object(dictionary! { "FT" => "Tx", "T" => lit("Name") });
        let cat_id = d.trailer.get(b"Root").unwrap().as_reference().unwrap();
        d.get_dictionary_mut(cat_id).unwrap().set(
            "AcroForm",
            dictionary! {
                "Fields" => Object::Array(vec![Object::Reference(field)]),
                "DA" => lit("/Helv 0 Tf 0 g"),
            },
        );
        let mut pdf = Vec::new();
        d.save_to(&mut pdf).unwrap();

        let meta = SigDictMeta { placeholder_len: 64, field_seq: 1, ..Default::default() };
        let out = Document::load_mem(&prepare(&pdf, &meta).unwrap().bytes).unwrap();
        let cat = out.get_dictionary(cat_id).unwrap();
        let af = out.get_dict_in_dict(cat, b"AcroForm").unwrap();
        let fields = af.get(b"Fields").unwrap().as_array().unwrap();
        assert_eq!(fields.len(), 2, "existing form field dropped");
        assert!(fields[0] == Object::Reference(field));
        assert!(af.has(b"DA"));
    }

    #[test]
    fn non_ascii_text_is_utf16() {
        assert!(text("Berlin") == lit("Berlin"));
        match text("M\u{fc}nchen") {
            Object::String(b, _) => {
                assert_eq!(&b[..4], &[0xFE, 0xFF, 0x00, b'M']);
                assert_eq!(&b[4..6], &[0x00, 0xFC]);
            }
            _ => panic!("not a string"),
        }
    }
}
