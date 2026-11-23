//! XMP metadata and the document information dictionary.
//!
//! The catalog metadata is rewritten from scratch with predefined schemas
//! only (dc, xmp, pdf, xmpMM, pdfaid), so no extension schemas are needed.
//! Info entries and their XMP equivalents get identical values (parts 1–3);
//! part 4 drops the Info dictionary.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

use crate::util::{self, deref};
use crate::Ctx;

#[derive(Default, Debug, Clone)]
pub struct Meta {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: String,
    pub create_date: Option<(String, String)>,
    pub modify_date: (String, String),
}

pub fn run(ctx: &mut Ctx) {
    let level = ctx.level;
    let producer = ctx.opts.producer.clone();
    let doc = &mut ctx.doc;

    // Metadata streams other than the catalog's are dropped: their schemas
    // would all have to be valid PDF/A metadata too.
    let cat_id = util::catalog_id(doc);
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        if Some(id) == cat_id {
            continue;
        }
        if let Some(d) = util::dict_mut(doc, id) {
            d.remove(b"Metadata");
        }
    }

    let info = read_info(doc);
    let (now_pdf, now_xmp) = util::now_dates();
    let meta = Meta {
        title: info.get_text(b"Title"),
        author: info.get_text(b"Author"),
        subject: info.get_text(b"Subject"),
        keywords: info.get_text(b"Keywords"),
        creator: info.get_text(b"Creator"),
        producer,
        create_date: info.get_text(b"CreationDate").and_then(|d| pdf_date_to_xmp(&d).map(|x| (normalise_pdf_date(&d), x))),
        modify_date: (now_pdf, now_xmp),
    };

    let packet = build_packet(&meta, level.part(), level.pdfaid_conformance(), document_id(doc));
    let mut s = Stream::new(dictionary! { "Type" => "Metadata", "Subtype" => "XML" }, packet.into_bytes());
    s.allows_compression = false;
    let mid = doc.add_object(Object::Stream(s));
    if let Some(c) = util::catalog_mut(doc) {
        c.set("Metadata", Object::Reference(mid));
    }

    if level.keeps_info_dict() {
        let mut d = Dictionary::new();
        // Keep custom keys, rewrite the standard ones consistently.
        for (k, v) in info.dict.iter() {
            let std = matches!(
                k.as_slice(),
                b"Title" | b"Author" | b"Subject" | b"Keywords" | b"Creator" | b"Producer" | b"CreationDate" | b"ModDate" | b"Trapped"
            );
            if !std {
                if let Object::String(..) = deref(doc, v) {
                    d.set(k.clone(), deref(doc, v).clone());
                }
            }
        }
        let set = |d: &mut Dictionary, k: &str, v: &Option<String>| {
            if let Some(v) = v {
                d.set(k, util::text_string(v));
            }
        };
        set(&mut d, "Title", &meta.title);
        set(&mut d, "Author", &meta.author);
        set(&mut d, "Subject", &meta.subject);
        set(&mut d, "Keywords", &meta.keywords);
        set(&mut d, "Creator", &meta.creator);
        d.set("Producer", util::text_string(&meta.producer));
        if let Some((pdf, _)) = &meta.create_date {
            d.set("CreationDate", Object::string_literal(pdf.as_str()));
        }
        d.set("ModDate", Object::string_literal(meta.modify_date.0.as_str()));
        let iid = doc.add_object(Object::Dictionary(d));
        doc.trailer.set("Info", Object::Reference(iid));
    } else {
        doc.trailer.remove(b"Info");
    }
}

struct Info {
    dict: Dictionary,
}

impl Info {
    fn get_text(&self, key: &[u8]) -> Option<String> {
        let o = self.dict.get(key).ok()?;
        let s = util::decode_text(o)?;
        let s: String = s.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').collect();
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    }
}

fn read_info(doc: &Document) -> Info {
    let mut dict = doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|o| util::dict_of(doc, o))
        .cloned()
        .unwrap_or_default();
    // Resolve indirect values.
    let keys: Vec<Vec<u8>> = dict.iter().map(|(k, _)| k.clone()).collect();
    for k in keys {
        if let Ok(v) = dict.get(&k) {
            let r = deref(doc, v).clone();
            dict.set(k, r);
        }
    }
    Info { dict }
}

fn document_id(doc: &Document) -> String {
    let seed = match doc.trailer.get(b"ID") {
        Ok(Object::Array(a)) => match a.first() {
            Some(Object::String(s, _)) => s.clone(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    let bytes = if seed.len() >= 16 { seed[..16].to_vec() } else { crate::write::random_id(&seed) };
    let h: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("uuid:{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

pub fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => {}
            '\u{FFFE}' | '\u{FFFF}' => {}
            c => o.push(c),
        }
    }
    o
}

pub fn build_packet(m: &Meta, part: u8, conformance: Option<&str>, doc_id: String) -> String {
    let mut x = String::new();
    x.push_str("<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n");
    x.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n");
    x.push_str(" <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");
    x.push_str("  <rdf:Description rdf:about=\"\"\n");
    x.push_str("    xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"\n");
    x.push_str("    xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n");
    x.push_str("    xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n");
    x.push_str("    xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\"\n");
    x.push_str("    xmlns:xmpMM=\"http://ns.adobe.com/xap/1.0/mm/\">\n");
    x.push_str(&format!("   <pdfaid:part>{part}</pdfaid:part>\n"));
    if let Some(c) = conformance {
        x.push_str(&format!("   <pdfaid:conformance>{c}</pdfaid:conformance>\n"));
    }
    if part == 4 {
        x.push_str("   <pdfaid:rev>2020</pdfaid:rev>\n");
    }
    x.push_str("   <dc:format>application/pdf</dc:format>\n");
    if let Some(t) = &m.title {
        x.push_str(&format!("   <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title>\n", escape(t)));
    }
    if let Some(a) = &m.author {
        x.push_str(&format!("   <dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>\n", escape(a)));
    }
    if let Some(s) = &m.subject {
        x.push_str(&format!("   <dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:description>\n", escape(s)));
    }
    if let Some(k) = &m.keywords {
        x.push_str(&format!("   <pdf:Keywords>{}</pdf:Keywords>\n", escape(k)));
    }
    x.push_str(&format!("   <pdf:Producer>{}</pdf:Producer>\n", escape(&m.producer)));
    if let Some(c) = &m.creator {
        x.push_str(&format!("   <xmp:CreatorTool>{}</xmp:CreatorTool>\n", escape(c)));
    }
    if let Some((_, d)) = &m.create_date {
        x.push_str(&format!("   <xmp:CreateDate>{d}</xmp:CreateDate>\n"));
    }
    x.push_str(&format!("   <xmp:ModifyDate>{}</xmp:ModifyDate>\n", m.modify_date.1));
    x.push_str(&format!("   <xmp:MetadataDate>{}</xmp:MetadataDate>\n", m.modify_date.1));
    x.push_str(&format!("   <xmpMM:DocumentID>{doc_id}</xmpMM:DocumentID>\n"));
    x.push_str("  </rdf:Description>\n");
    x.push_str(" </rdf:RDF>\n");
    x.push_str("</x:xmpmeta>\n");
    // Padding so the packet can be edited in place.
    for _ in 0..20 {
        x.push_str(&" ".repeat(99));
        x.push('\n');
    }
    x.push_str("<?xpacket end=\"w\"?>");
    x
}

/// Parse a PDF date ("D:YYYYMMDDHHmmSSOHH'mm'") into its parts.
fn parse_pdf_date(s: &str) -> Option<(u32, u32, u32, u32, u32, u32, Option<(char, u32, u32)>)> {
    let s = s.trim();
    let s = s.strip_prefix("D:").unwrap_or(s);
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 {
        return None;
    }
    let num = |a: usize, b: usize, def: u32| -> u32 {
        digits.get(a..b).and_then(|x| x.parse().ok()).unwrap_or(def)
    };
    let (y, mo, d, h, mi, se) = (num(0, 4, 0), num(4, 6, 1), num(6, 8, 1), num(8, 10, 0), num(10, 12, 0), num(12, 14, 0));
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 59 {
        return None;
    }
    let rest = &s[digits.len()..];
    let tz = match rest.chars().next() {
        Some('Z') => Some(('Z', 0, 0)),
        Some(c @ ('+' | '-')) => {
            let tzd: String = rest[1..].chars().filter(|c| c.is_ascii_digit()).collect();
            let th = tzd.get(0..2).and_then(|x| x.parse().ok()).unwrap_or(0);
            let tm = tzd.get(2..4).and_then(|x| x.parse().ok()).unwrap_or(0);
            if th > 23 || tm > 59 {
                None
            } else {
                Some((c, th, tm))
            }
        }
        _ => None,
    };
    Some((y, mo, d, h, mi, se, tz))
}

/// PDF date → XMP date with the same instant and offset.
pub fn pdf_date_to_xmp(s: &str) -> Option<String> {
    let (y, mo, d, h, mi, se, tz) = parse_pdf_date(s)?;
    let tzs = match tz {
        Some(('Z', _, _)) => "Z".to_string(),
        Some((c, th, tm)) => format!("{c}{th:02}:{tm:02}"),
        None => "Z".to_string(),
    };
    Some(format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{se:02}{tzs}"))
}

/// A well-formed PDF date equivalent to the XMP value produced above.
fn normalise_pdf_date(s: &str) -> String {
    match parse_pdf_date(s) {
        Some((y, mo, d, h, mi, se, tz)) => {
            let tzs = match tz {
                Some(('Z', _, _)) | None => "Z".to_string(),
                Some((c, th, tm)) => format!("{c}{th:02}'{tm:02}'"),
            };
            format!("D:{y:04}{mo:02}{d:02}{h:02}{mi:02}{se:02}{tzs}")
        }
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(pdf_date_to_xmp("D:20240102030405+01'00'").unwrap(), "2024-01-02T03:04:05+01:00");
        assert_eq!(pdf_date_to_xmp("D:20240102").unwrap(), "2024-01-02T00:00:00Z");
        assert_eq!(pdf_date_to_xmp("D:2024010203Z").unwrap(), "2024-01-02T03:00:00Z");
        assert_eq!(normalise_pdf_date("D:20240102030405-05'30"), "D:20240102030405-05'30'");
        assert_eq!(normalise_pdf_date("D:2024"), "D:20240101000000Z");
        assert!(pdf_date_to_xmp("D:20241302").is_none());
        assert!(pdf_date_to_xmp("garbage").is_none());
        assert_eq!(normalise_pdf_date("garbage"), "garbage");
    }

    #[test]
    fn packet() {
        let m = Meta {
            title: Some("A & <B>".into()),
            author: Some("Me".into()),
            subject: Some("S".into()),
            keywords: Some("k".into()),
            creator: Some("c".into()),
            producer: "P".into(),
            create_date: Some(("D:2024".into(), "2024-01-01T00:00:00Z".into())),
            modify_date: ("D:2025".into(), "2025-01-01T00:00:00Z".into()),
        };
        let p = build_packet(&m, 2, Some("B"), "uuid:x".into());
        assert!(p.contains("<pdfaid:part>2</pdfaid:part>"));
        assert!(p.contains("<pdfaid:conformance>B</pdfaid:conformance>"));
        assert!(p.contains("A &amp; &lt;B&gt;"));
        assert!(!p.contains("pdfaid:rev"));
        assert!(!p.contains("bytes=") && !p.contains("encoding="));
        let p4 = build_packet(&m, 4, None, "uuid:x".into());
        assert!(p4.contains("<pdfaid:rev>2020</pdfaid:rev>") && !p4.contains("conformance"));
        assert_eq!(escape("a\u{1}b\"\u{FFFE}"), "ab&quot;");
    }
}
