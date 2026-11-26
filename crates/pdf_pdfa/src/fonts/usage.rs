//! Which character codes each font shows, and in which rendering mode.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::cmap::{self, CMap, Code};
use crate::content::{self, Operand};
use crate::util::{self, deref};

#[derive(Default, Debug, Clone)]
pub struct FontUse {
    /// Every code shown with the font.
    pub codes: BTreeSet<Code>,
    /// Codes shown in a visible rendering mode (not 3).
    pub rendered: BTreeSet<Code>,
}

/// The code splitter (CMap) for a font dictionary.
pub fn font_cmap(doc: &Document, font: &Dictionary) -> CMap {
    if util::get_name(doc, font, b"Subtype") != Some(b"Type0".as_slice()) {
        return CMap::one_byte();
    }
    match font.get(b"Encoding").ok().map(|o| deref(doc, o)) {
        Some(Object::Name(n)) => cmap::predefined(n).unwrap_or_else(CMap::identity),
        Some(Object::Stream(s)) => {
            let mut m = util::stream_data(s).map(|d| CMap::parse(&d)).unwrap_or_else(CMap::identity);
            if m.codespace.is_empty() || (m.cid_ranges.is_empty() && m.usecmap.is_some()) {
                if let Some(base) = s.dict.get(b"UseCMap").ok().and_then(|o| o.as_name().ok()).map(|n| n.to_vec()).or(m.usecmap.clone()) {
                    if let Some(p) = cmap::predefined(&base) {
                        if m.codespace.is_empty() {
                            m.codespace = p.codespace.clone();
                        }
                        if m.cid_ranges.is_empty() {
                            m.cid_ranges = p.cid_ranges.clone();
                        }
                    }
                }
            }
            if m.codespace.is_empty() {
                m.codespace = CMap::identity().codespace;
            }
            m
        }
        _ => CMap::identity(),
    }
}

struct Walker<'a> {
    doc: &'a Document,
    uses: BTreeMap<ObjectId, FontUse>,
    cmaps: BTreeMap<ObjectId, CMap>,
}

#[derive(Clone, Copy)]
struct State {
    font: Option<ObjectId>,
    mode: i64,
}

impl Walker<'_> {
    fn record(&mut self, font: Option<ObjectId>, mode: i64, s: &[u8]) {
        let Some(fid) = font else { return };
        let doc = self.doc;
        let cm = self.cmaps.entry(fid).or_insert_with(|| match doc.objects.get(&fid) {
            Some(Object::Dictionary(d)) => font_cmap(doc, d),
            _ => CMap::one_byte(),
        });
        let codes = cm.split(s);
        let u = self.uses.entry(fid).or_default();
        for c in codes {
            u.codes.insert(c);
            if mode != 3 && mode != 7 {
                u.rendered.insert(c);
            }
        }
    }

    fn walk(&mut self, data: &[u8], res: Option<&Dictionary>) {
        let doc = self.doc;
        let fonts = res.and_then(|r| util::get_dict(doc, r, b"Font"));
        let gstates = res.and_then(|r| util::get_dict(doc, r, b"ExtGState"));
        let mut st = State { font: None, mode: 0 };
        let mut stack: Vec<State> = Vec::new();
        let mut shows: Vec<(Option<ObjectId>, i64, Vec<u8>)> = Vec::new();
        content::visit(data, |op| {
            match op.name {
                b"q" => stack.push(st),
                b"Q" => {
                    if let Some(s) = stack.pop() {
                        st = s;
                    }
                }
                b"Tf" => {
                    if let Some(n) = op.args.first().and_then(|a| a.as_name()) {
                        st.font = fonts.and_then(|f| f.get(n).ok()).and_then(|o| o.as_reference().ok());
                    }
                }
                b"Tr" => {
                    if let Some(m) = op.args.first().and_then(|a| a.as_f64()) {
                        st.mode = m as i64;
                    }
                }
                b"gs" => {
                    if let Some(n) = op.args.first().and_then(|a| a.as_name()) {
                        if let Some(g) = gstates.and_then(|g| util::get_dict(doc, g, n)) {
                            if let Some(Object::Array(a)) = g.get(b"Font").ok() {
                                if let Some(Object::Reference(r)) = a.first() {
                                    st.font = Some(*r);
                                }
                            }
                        }
                    }
                }
                b"Tj" | b"'" => {
                    if let Some(Operand::Str(s)) = op.args.first() {
                        shows.push((st.font, st.mode, s.clone()));
                    }
                }
                b"\"" => {
                    if let Some(Operand::Str(s)) = op.args.get(2) {
                        shows.push((st.font, st.mode, s.clone()));
                    }
                }
                b"TJ" => {
                    if let Some(Operand::Array(a)) = op.args.first() {
                        for x in a {
                            if let Operand::Str(s) = x {
                                shows.push((st.font, st.mode, s.clone()));
                            }
                        }
                    }
                }
                _ => {}
            }
            true
        });
        for (f, m, s) in shows {
            self.record(f, m, &s);
        }
    }
}

/// Walk every content stream in the document.
pub fn collect(doc: &Document) -> BTreeMap<ObjectId, FontUse> {
    let mut w = Walker { doc, uses: BTreeMap::new(), cmaps: BTreeMap::new() };
    for page in util::pages(doc) {
        let res = util::inherited(doc, page, b"Resources").and_then(|o| util::dict_of(doc, &o).cloned());
        let data = util::page_content(doc, page);
        w.walk(&data, res.as_ref());
    }
    for obj in doc.objects.values() {
        match obj {
            Object::Stream(s) => {
                let is_form = s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Form".as_slice());
                let is_pattern = s.dict.get(b"PatternType").ok().and_then(|o| o.as_i64().ok()) == Some(1);
                if is_form || is_pattern {
                    if let Some(data) = util::stream_data(s) {
                        let res = util::get_dict(doc, &s.dict, b"Resources").cloned();
                        w.walk(&data, res.as_ref());
                    }
                }
            }
            Object::Dictionary(d) if d.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Type3".as_slice()) => {
                let res = util::get_dict(doc, d, b"Resources").cloned();
                if let Some(cp) = util::get_dict(doc, d, b"CharProcs") {
                    for (_, v) in cp.iter() {
                        if let Object::Stream(s) = deref(doc, v) {
                            if let Some(data) = util::stream_data(s) {
                                w.walk(&data, res.as_ref());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    w.uses
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    #[test]
    fn collects_codes_and_modes() {
        let mut doc = Document::with_version("1.7");
        let f1 = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
        let f2 = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "X", "Encoding" => "Identity-H" });
        let content = b"BT /F1 12 Tf (AB) Tj 3 Tr (C) Tj ET q BT /F2 9 Tf 0 Tr [<00410042> -10 <0043>] TJ ET Q";
        let cid = doc.add_object(Stream::new(Dictionary::new(), content.to_vec()));
        let pages = doc.new_object_id();
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages, "Contents" => cid,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => f1, "F2" => f2 } },
        });
        doc.objects.insert(pages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
        let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", cat);
        let u = collect(&doc);
        let a = &u[&f1];
        assert_eq!(a.codes.len(), 3);
        assert_eq!(a.rendered.len(), 2);
        assert!(!a.rendered.contains(&(b'C' as u32, 1)));
        let b = &u[&f2];
        assert!(b.rendered.contains(&(0x41, 2)) && b.rendered.contains(&(0x43, 2)));
    }
}
