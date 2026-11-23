//! Image and form XObject dictionary rules.

use lopdf::{Dictionary, Object, ObjectId, Stream};

use crate::util;
use crate::{Ctx, Result};

pub fn run(ctx: &mut Ctx) -> Result<()> {
    let part = ctx.level.part();
    let doc = &mut ctx.doc;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut ps_removed = 0;
    for id in ids {
        let Some(Object::Stream(s)) = doc.objects.get_mut(&id) else { continue };
        let sub = s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()).map(|n| n.to_vec());
        match sub.as_deref() {
            Some(b"Image") => {
                s.dict.remove(b"Alternates");
                s.dict.remove(b"OPI");
                s.dict.remove(b"Interpolate");
                let mask = matches!(s.dict.get(b"ImageMask"), Ok(Object::Boolean(true)));
                if mask {
                    if s.dict.has(b"BitsPerComponent") {
                        s.dict.set("BitsPerComponent", 1);
                    }
                } else if let Ok(bpc) = s.dict.get(b"BitsPerComponent").and_then(|o| o.as_i64()) {
                    let ok = matches!(bpc, 1 | 2 | 4 | 8) || (bpc == 16 && part > 1);
                    if !ok && bpc == 16 {
                        to_8bit(s);
                    }
                }
            }
            Some(b"Form") => {
                s.dict.remove(b"OPI");
                s.dict.remove(b"PS");
                s.dict.remove(b"Ref");
                if matches!(s.dict.get(b"Subtype2"), Ok(Object::Name(n)) if n == b"PS") {
                    s.dict.remove(b"Subtype2");
                }
            }
            Some(b"PS") => {
                // PostScript XObjects are ignored by viewers anyway.
                let mut d = Dictionary::new();
                d.set("Type", Object::Name(b"XObject".to_vec()));
                d.set("Subtype", Object::Name(b"Form".to_vec()));
                d.set("BBox", Object::Array(vec![0.into(), 0.into(), 0.into(), 0.into()]));
                *s = Stream::new(d, Vec::new());
                ps_removed += 1;
            }
            _ => {}
        }
    }
    if ps_removed > 0 {
        ctx.report.note("postscript_removed", format!("{ps_removed}"));
    }
    Ok(())
}

/// 16-bit samples → 8-bit (PDF/A-1 allows at most 8). Only possible when
/// the data can be decoded (Flate/LZW/uncompressed).
fn to_8bit(s: &mut Stream) {
    let Some(data) = util::stream_data(s) else { return };
    let out: Vec<u8> = data.chunks(2).map(|c| c[0]).collect();
    let mut dict = s.dict.clone();
    dict.set("BitsPerComponent", 8);
    *s = util::flate_stream(dict, &out);
}
