//! Embedded font programs that carry more than the document uses.
//!
//! Fonts that are already embedded and valid stay as they are in the font
//! dictionary: same codes, widths, encoding and ToUnicode map. Only the
//! font program gets smaller:
//!
//! - TrueType: tables no PDF viewer reads (layout, kerning, bitmaps,
//!   signature) are left out and the outlines of unused glyphs removed.
//!   Glyph ids, `cmap`, `post`, metrics and hinting stay.
//! - CFF: the outlines of unused glyphs are emptied in place.
//!
//! A program is only replaced when every glyph the document uses has the
//! same outline and advance afterwards. Anything the pass cannot map with
//! certainty (unknown encodings, fonts the form fields may still need for
//! new input, Type 1 programs) is left alone.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Dictionary, Document, Object, ObjectId};
use ttf_parser::GlyphId;

use super::{cff, embed, encoding, sfnt, usage, FontUse};
use crate::util::{self, deref, dict_mut};
use crate::Ctx;

/// A font dictionary that draws with an embedded program.
struct User {
    /// The dictionary text is shown with (simple font or Type0).
    font: ObjectId,
    /// The CIDFont below a Type0 font.
    cid_font: Option<ObjectId>,
    descriptor: ObjectId,
}

#[derive(Default)]
struct File {
    users: Vec<User>,
    /// Some use of the program is not known: hands off.
    unknown_use: bool,
}

/// Every stream that is the font program of some font descriptor.
pub fn font_files(doc: &Document) -> BTreeSet<ObjectId> {
    let mut out = BTreeSet::new();
    for o in doc.objects.values() {
        if let Object::Dictionary(d) = o {
            if let Some((_, id)) = file_of(d) {
                out.insert(id);
            }
        }
    }
    out
}

fn file_of(descriptor: &Dictionary) -> Option<(&'static [u8], ObjectId)> {
    for key in [b"FontFile2".as_slice(), b"FontFile3"] {
        if let Ok(Object::Reference(id)) = descriptor.get(key) {
            return Some((key, *id));
        }
    }
    None
}

fn subtype<'a>(d: &'a Dictionary) -> &'a [u8] {
    match d.get(b"Subtype") {
        Ok(Object::Name(n)) => n,
        _ => b"",
    }
}

/// Fonts the interactive form names for new input (`/DR`): a viewer may
/// draw any character with them.
fn form_fonts(doc: &Document) -> BTreeSet<ObjectId> {
    let mut out = BTreeSet::new();
    let fonts = util::catalog(doc)
        .and_then(|c| util::get_dict(doc, c, b"AcroForm"))
        .and_then(|a| util::get_dict(doc, a, b"DR"))
        .and_then(|r| util::get_dict(doc, r, b"Font"));
    if let Some(fonts) = fonts {
        for (_, v) in fonts.iter() {
            if let Object::Reference(id) = v {
                out.insert(*id);
            }
        }
    }
    out
}

/// Font programs of the input (`original`) with the fonts that use them.
fn collect(doc: &Document, uses: &BTreeMap<ObjectId, FontUse>, original: &BTreeSet<ObjectId>) -> BTreeMap<ObjectId, File> {
    let form = form_fonts(doc);
    let mut files: BTreeMap<ObjectId, File> = BTreeMap::new();
    for (&id, o) in &doc.objects {
        let Object::Dictionary(d) = o else { continue };
        let (holder, cid_font) = match subtype(d) {
            b"TrueType" | b"Type1" | b"MMType1" => (d, None),
            b"Type0" => {
                let first = util::get_array(doc, d, b"DescendantFonts").and_then(|a| a.first());
                match first {
                    Some(Object::Reference(cf)) => match doc.objects.get(cf) {
                        Some(Object::Dictionary(c)) => (c, Some(*cf)),
                        _ => continue,
                    },
                    _ => continue,
                }
            }
            _ => continue,
        };
        let Ok(Object::Reference(descriptor)) = holder.get(b"FontDescriptor") else { continue };
        let Some(Object::Dictionary(desc)) = doc.objects.get(descriptor) else { continue };
        let Some((_, file)) = file_of(desc) else { continue };
        if !original.contains(&file) {
            continue;
        }
        let entry = files.entry(file).or_default();
        if !uses.contains_key(&id) || form.contains(&id) {
            entry.unknown_use = true;
        }
        entry.users.push(User { font: id, cid_font, descriptor: *descriptor });
    }
    files
}

pub fn run(ctx: &mut Ctx, uses: &BTreeMap<ObjectId, FontUse>, original: &BTreeSet<ObjectId>) {
    let files = collect(&ctx.doc, uses, original);
    let mut saved = 0usize;
    let mut count = 0usize;
    for (file, info) in &files {
        if info.unknown_use {
            continue;
        }
        if let Some(bytes) = shrink_file(&mut ctx.doc, *file, info, uses) {
            saved += bytes;
            count += 1;
        }
    }
    if count > 0 {
        ctx.report.note("fonts_reduced", format!("{count} ({saved} bytes)"));
    }
}

/// The glyph outlines of a font program, for comparing before and after.
#[derive(Default, PartialEq, Debug)]
struct Path(Vec<f32>);

impl ttf_parser::OutlineBuilder for Path {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.extend_from_slice(&[0.0, x, y]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.extend_from_slice(&[1.0, x, y]);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.extend_from_slice(&[2.0, x1, y1, x, y]);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.extend_from_slice(&[3.0, x1, y1, x2, y2, x, y]);
    }
    fn close(&mut self) {
        self.0.push(4.0);
    }
}

/// A font program as the pass sees it.
enum Program<'a> {
    /// TrueType outlines (FontFile2, or OpenType with a glyf table).
    TrueType(ttf_parser::Face<'a>),
    /// CFF, bare or inside an OpenType file; the range of the CFF data.
    Cff(ttf_parser::cff::Table<'a>, (usize, usize)),
}

impl<'a> Program<'a> {
    fn parse(data: &'a [u8]) -> Option<Program<'a>> {
        if let Ok(face) = ttf_parser::Face::parse(data, 0) {
            if face.tables().glyf.is_some() {
                return Some(Program::TrueType(face));
            }
            let range = sfnt::Sfnt::parse(data, 0).ok()?.table_range(b"CFF ")?;
            let table = face.tables().cff?;
            return Some(Program::Cff(table, range));
        }
        let table = ttf_parser::cff::Table::parse(data)?;
        Some(Program::Cff(table, (0, data.len())))
    }

    fn glyphs(&self) -> u16 {
        match self {
            Program::TrueType(f) => f.number_of_glyphs(),
            Program::Cff(t, _) => t.number_of_glyphs(),
        }
    }

    /// Outline and advance of a glyph.
    fn glyph(&self, gid: u16) -> (Path, Option<u16>) {
        let mut path = Path::default();
        match self {
            Program::TrueType(f) => {
                let _ = f.outline_glyph(GlyphId(gid), &mut path);
                (path, f.glyph_hor_advance(GlyphId(gid)))
            }
            Program::Cff(t, _) => {
                let _ = t.outline(GlyphId(gid), &mut path);
                (path, t.glyph_width(GlyphId(gid)))
            }
        }
    }
}

/// Glyphs `user` shows, or `None` when that cannot be told for sure.
fn used_glyphs(doc: &Document, user: &User, u: &FontUse, program: &Program) -> Option<Vec<u16>> {
    let Some(Object::Dictionary(d)) = doc.objects.get(&user.font) else { return None };
    let mut out = Vec::new();
    if let Some(cf_id) = user.cid_font {
        let Some(Object::Dictionary(cf)) = doc.objects.get(&cf_id) else { return None };
        let identity = match d.get(b"Encoding").ok().map(|o| deref(doc, o)) {
            Some(Object::Name(n)) if n == b"Identity-H" || n == b"Identity-V" => true,
            // An embedded CMap has to name the CID of every code.
            Some(Object::Stream(_)) => false,
            _ => return None,
        };
        let cmap = usage::font_cmap(doc, d);
        // CID → glyph.
        let truetype_map = match (program, subtype(cf)) {
            (Program::TrueType(_), b"CIDFontType2") => match cf.get(b"CIDToGIDMap").ok().map(|o| deref(doc, o)) {
                Some(Object::Stream(s)) => {
                    let data = util::stream_data(s)?;
                    let mut map = Vec::with_capacity(data.len() / 2);
                    for pair in data.chunks_exact(2) {
                        map.push(u16::from_be_bytes([pair[0], pair[1]]));
                    }
                    Some(map)
                }
                Some(Object::Name(n)) if n == b"Identity" => None,
                None => None,
                Some(_) => return None,
            },
            (Program::Cff(..), b"CIDFontType0") => None,
            _ => return None,
        };
        let mut cff_map: BTreeMap<u32, u16> = BTreeMap::new();
        if let Program::Cff(t, _) = program {
            for g in 0..t.number_of_glyphs() {
                // Without a CID-keyed charset the glyph index is the CID.
                let cid = t.glyph_cid(GlyphId(g)).unwrap_or(g) as u32;
                cff_map.insert(cid, g);
            }
        }
        for &code in &u.codes {
            let cid = match cmap.cid(code) {
                Some(c) => c,
                None if identity => code.0,
                None => return None,
            };
            let gid = match program {
                Program::Cff(..) => cff_map.get(&cid).copied().unwrap_or(0),
                Program::TrueType(_) => match &truetype_map {
                    Some(m) => m.get(cid as usize).copied().unwrap_or(0),
                    None => u16::try_from(cid).ok()?,
                },
            };
            out.push(gid);
        }
        return Some(out);
    }
    match (program, subtype(d)) {
        (Program::TrueType(face), b"TrueType") => {
            let names = encoding::simple_encoding(doc, d, None);
            let flags = util::get_dict(doc, d, b"FontDescriptor").and_then(|x| util::get_num(doc, x, b"Flags")).unwrap_or(32.0) as i64;
            for &(code, _) in &u.codes {
                out.push(super::truetype_gid(face, code as u8, names[code as usize & 0xFF].as_deref(), flags & 4 != 0));
            }
        }
        (Program::Cff(t, _), b"Type1" | b"MMType1") => {
            let mut builtin: [Option<String>; 256] = std::array::from_fn(|_| None);
            for (code, slot) in builtin.iter_mut().enumerate() {
                if let Some(g) = t.glyph_index(code as u8) {
                    *slot = t.glyph_name(g).map(str::to_string);
                }
            }
            let names = encoding::simple_encoding(doc, d, Some(&builtin));
            let mut by_name: BTreeMap<&str, u16> = BTreeMap::new();
            for g in 0..t.number_of_glyphs() {
                if let Some(n) = t.glyph_name(GlyphId(g)) {
                    by_name.entry(n).or_insert(g);
                }
            }
            for &(code, _) in &u.codes {
                if let Some(n) = names[code as usize & 0xFF].as_deref() {
                    out.push(by_name.get(n).copied().unwrap_or(0));
                }
            }
        }
        _ => return None,
    }
    Some(out)
}

/// Every kept glyph looks and measures the same in both programs.
fn same_glyphs(old: &Program, new: &Program, keep: &BTreeSet<u16>) -> bool {
    if old.glyphs() != new.glyphs() {
        return false;
    }
    for &g in keep {
        if old.glyph(g) != new.glyph(g) {
            return false;
        }
    }
    true
}

/// A smaller program with the same glyphs in `keep`, and how many
/// outlines it lost. `None` when the program cannot be reduced safely.
fn reduce(data: &[u8], keep: &BTreeSet<u16>) -> Option<(Vec<u8>, usize)> {
    let old = Program::parse(data)?;
    let (new, removed) = match &old {
        Program::TrueType(_) => sfnt::slim(&sfnt::Sfnt::parse(data, 0).ok()?, keep).ok()?,
        Program::Cff(_, (a, b)) => {
            let mut copy = data.to_vec();
            let removed = cff::blank(&mut copy[*a..*b], keep)?;
            (copy, removed)
        }
    };
    let ok = match Program::parse(&new) {
        Some(parsed) => same_glyphs(&old, &parsed, keep),
        None => false,
    };
    ok.then_some((new, removed))
}

/// Give a name the subset tag, unless it has one.
fn tag_name(doc: &mut Document, id: ObjectId, key: &[u8], tag: &str) {
    let Some(d) = dict_mut(doc, id) else { return };
    let Ok(Object::Name(n)) = d.get(key) else { return };
    let name = String::from_utf8_lossy(n).to_string();
    if encoding::strip_subset(&name) == name {
        d.set(key, Object::Name(format!("{tag}+{name}").into_bytes()));
    }
}

/// Replace the program `file` by a reduced one. Returns the bytes saved.
fn shrink_file(doc: &mut Document, file: ObjectId, info: &File, uses: &BTreeMap<ObjectId, FontUse>) -> Option<usize> {
    let Some(Object::Stream(stream)) = doc.objects.get(&file) else { return None };
    let old_len = stream.content.len();
    let data = util::stream_data(stream)?;
    let mut keep: BTreeSet<u16> = BTreeSet::from([0]);
    {
        let program = Program::parse(&data)?;
        for user in &info.users {
            keep.extend(used_glyphs(doc, user, uses.get(&user.font)?, &program)?);
        }
    }
    let (new, removed) = reduce(&data, &keep)?;
    let mut dict = stream.dict.clone();
    if dict.has(b"Length1") {
        dict.set("Length1", new.len() as i64);
    }
    let replacement = util::flate_stream(dict, &new);
    let new_len = replacement.content.len();
    // Not worth a change: less than 64 bytes or 5 % smaller.
    if new_len + 64 > old_len || new_len * 20 > old_len * 19 {
        return None;
    }
    doc.objects.insert(file, Object::Stream(replacement));
    if removed > 0 {
        // The program is a subset now and has to say so in its name.
        let tag = embed::subset_tag(&new[..new.len().min(4096)]);
        for user in &info.users {
            tag_name(doc, user.descriptor, b"FontName", &tag);
            tag_name(doc, user.font, b"BaseFont", &tag);
            if let Some(cf) = user.cid_font {
                tag_name(doc, cf, b"BaseFont", &tag);
            }
        }
    }
    Some(old_len - new_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{self, B};
    use lopdf::{dictionary, Stream};

    fn roboto() -> Vec<u8> {
        pdf_fonts::family("Roboto").unwrap().face(false, false).to_vec()
    }

    fn name(s: &str) -> Object {
        Object::Name(s.as_bytes().to_vec())
    }

    /// Descriptor with `key` pointing at a new font file stream.
    fn descriptor(doc: &mut Document, font_name: &str, key: &str, file: Stream) -> (ObjectId, ObjectId) {
        let file = doc.add_object(file);
        let mut d = dictionary! { "Type" => "FontDescriptor", "FontName" => name(font_name), "Flags" => 32 };
        d.set(key, file);
        (doc.add_object(d), file)
    }

    fn use_of(codes: &[(u32, u8)]) -> FontUse {
        let mut u = FontUse::default();
        for c in codes {
            u.codes.insert(*c);
            u.rendered.insert(*c);
        }
        u
    }

    fn program_of(doc: &Document, file: ObjectId) -> Vec<u8> {
        util::stream_data(doc.objects.get(&file).unwrap().as_stream().unwrap()).unwrap()
    }

    fn outlined(data: &[u8]) -> usize {
        let p = Program::parse(data).unwrap();
        (0..p.glyphs()).filter(|g| !p.glyph(*g).0 .0.is_empty()).count()
    }

    #[test]
    fn truetype_programs_lose_unused_glyphs_and_tables() {
        let font = roboto();
        let face = ttf_parser::Face::parse(&font, 0).unwrap();
        let (ga, gb) = (face.glyph_index('A').unwrap().0, face.glyph_index('B').unwrap().0);
        let mut doc = Document::with_version("1.7");
        let (desc, file) = descriptor(&mut doc, "Roboto-Regular", "FontFile2", Stream::new(dictionary! { "Length1" => font.len() as i64 }, font.clone()));
        // A simple font and a composite font share the program.
        let simple = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Roboto-Regular", "Encoding" => "WinAnsiEncoding", "FontDescriptor" => desc });
        let cid = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "Roboto-Regular", "FontDescriptor" => desc, "CIDToGIDMap" => "Identity" });
        let composite = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "Roboto-Regular", "Encoding" => "Identity-H", "DescendantFonts" => vec![cid.into()] });
        let mut uses = BTreeMap::new();
        uses.insert(simple, use_of(&[(b'A' as u32, 1)]));
        uses.insert(composite, use_of(&[(gb as u32, 2)]));
        let original = font_files(&doc);
        assert_eq!(original, BTreeSet::from([file]));

        let o = crate::Options::new("2b".parse().unwrap());
        let mut ctx = Ctx { doc, level: o.level, opts: &o, report: crate::Report::default(), source: &[] };
        run(&mut ctx, &uses, &original);
        assert!(ctx.report.has("fonts_reduced"), "{:?}", ctx.report.notes);
        let doc = ctx.doc;
        let new = program_of(&doc, file);
        assert!(new.len() * 4 < font.len(), "{} of {}", new.len(), font.len());
        let slim = ttf_parser::Face::parse(&new, 0).unwrap();
        assert_eq!(slim.number_of_glyphs(), face.number_of_glyphs());
        assert_eq!(slim.glyph_index('A').map(|g| g.0), Some(ga));
        assert_eq!(slim.glyph_hor_advance(GlyphId(gb)), face.glyph_hor_advance(GlyphId(gb)));
        // .notdef, A and B keep their outlines; nothing else does.
        assert_eq!(outlined(&new), 3);
        assert!(slim.tables().kern.is_none() && slim.raw_face().table(ttf_parser::Tag::from_bytes(b"GPOS")).is_none());
        let s = doc.objects.get(&file).unwrap().as_stream().unwrap();
        assert_eq!(s.dict.get(b"Length1").unwrap().as_i64().unwrap(), new.len() as i64);
        // Subset names everywhere, once.
        for (id, key) in [(desc, b"FontName".as_slice()), (simple, b"BaseFont"), (cid, b"BaseFont"), (composite, b"BaseFont")] {
            let n = String::from_utf8_lossy(doc.objects.get(&id).unwrap().as_dict().unwrap().get(key).unwrap().as_name().unwrap()).to_string();
            assert!(n.len() == "ABCDEF+Roboto-Regular".len() && n.ends_with("+Roboto-Regular"), "{n}");
        }

        // A second pass has nothing to gain and changes nothing.
        let mut ctx = Ctx { doc, level: o.level, opts: &o, report: crate::Report::default(), source: &[] };
        run(&mut ctx, &uses, &original);
        assert!(ctx.report.notes.is_empty());
        assert_eq!(program_of(&ctx.doc, file), new);
    }

    #[test]
    fn cff_programs_lose_unused_outlines() {
        let glyphs: Vec<(char, i32)> = ('A'..='Z').map(|c| (c, 500 + (c as i32 * 37) % 300)).collect();
        let cff = common::minimal_cff("DocuraWide", &glyphs);
        let mut doc = Document::with_version("1.7");
        // Bare CFF behind a simple font.
        let (desc, file) = descriptor(&mut doc, "DocuraWide", "FontFile3", Stream::new(dictionary! { "Subtype" => "Type1C" }, cff.clone()));
        let simple = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "DocuraWide", "Encoding" => "WinAnsiEncoding", "FontDescriptor" => desc });
        // The same program in an OpenType file behind a CID font.
        let otf = common::edge::opentype_with_cff(&cff, &vec![500; 27]);
        let (desc2, file2) = descriptor(&mut doc, "DocuraWide", "FontFile3", Stream::new(dictionary! { "Subtype" => "OpenType" }, otf.clone()));
        let cid = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "CIDFontType0", "BaseFont" => "DocuraWide", "FontDescriptor" => desc2 });
        let composite = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "DocuraWide-Identity-H", "Encoding" => "Identity-V", "DescendantFonts" => vec![cid.into()] });
        let mut uses = BTreeMap::new();
        // "B", "C" and a code without a glyph name.
        uses.insert(simple, use_of(&[(b'B' as u32, 1), (b'C' as u32, 1), (1, 1)]));
        uses.insert(composite, use_of(&[(3, 2), (500, 2)]));
        let original = font_files(&doc);
        for (f, user) in [(file, simple), (file2, composite)] {
            let info = collect(&doc, &uses, &original).remove(&f).unwrap();
            assert!(!info.unknown_use && info.users.len() == 1 && info.users[0].font == user);
            assert!(shrink_file(&mut doc, f, &info, &uses).is_some());
        }
        let new = program_of(&doc, file);
        assert_eq!(new.len(), cff.len());
        assert_eq!(outlined(&cff), 26);
        assert_eq!(outlined(&new), 2);
        let (old_t, new_t) = (ttf_parser::cff::Table::parse(&cff).unwrap(), ttf_parser::cff::Table::parse(&new).unwrap());
        assert_eq!(new_t.glyph_width(GlyphId(2)), old_t.glyph_width(GlyphId(2)));
        assert_eq!(new_t.glyph_name(GlyphId(26)), Some("Z"));
        let new2 = program_of(&doc, file2);
        assert_eq!(new2.len(), otf.len());
        assert_eq!(outlined(&new2), 1);
        let n = String::from_utf8_lossy(doc.objects.get(&composite).unwrap().as_dict().unwrap().get(b"BaseFont").unwrap().as_name().unwrap()).to_string();
        assert!(n.ends_with("+DocuraWide-Identity-H"), "{n}");
        assert!(doc.objects.get(&file).unwrap().as_stream().unwrap().dict.get(b"Length1").is_err());
    }

    #[test]
    fn uncertain_cases_are_left_alone() {
        let font = roboto();
        let cff = common::minimal_cff("X", &[('A', 600), ('B', 700)]);
        let mut doc = Document::with_version("1.7");
        let ttf = |doc: &mut Document| descriptor(doc, "Roboto-Regular", "FontFile2", Stream::new(dictionary! { "Length1" => font.len() as i64 }, font.clone()));
        let one = use_of(&[(65, 1)]);
        let wide = use_of(&[(70000, 2)]);
        let mut uses = BTreeMap::new();
        let mut expect_none: Vec<(ObjectId, &str)> = Vec::new();

        // Composite fonts whose codes cannot be mapped to glyphs for sure.
        let composite = |doc: &mut Document, uses: &mut BTreeMap<ObjectId, FontUse>, cid: Dictionary, encoding: Object, u: &FontUse| -> ObjectId {
            let (desc, file) = if subtype(&cid) == b"CIDFontType0" {
                descriptor(doc, "X", "FontFile3", Stream::new(dictionary! { "Subtype" => "CIDFontType0C" }, cff.clone()))
            } else {
                ttf(doc)
            };
            let mut cid = cid;
            cid.set("FontDescriptor", desc);
            let cid = doc.add_object(cid);
            let f = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0", "Encoding" => encoding, "DescendantFonts" => vec![cid.into()] });
            uses.insert(f, u.clone());
            file
        };
        let type2 = || dictionary! { "Type" => "Font", "Subtype" => "CIDFontType2" };
        expect_none.push((composite(&mut doc, &mut uses, type2(), name("UniJIS-UTF16-H"), &one), "predefined CMap"));
        let empty_cmap = doc.add_object(Stream::new(dictionary! { "Type" => "CMap" }, b"1 begincodespacerange <0000> <FFFF> endcodespacerange".to_vec()));
        expect_none.push((composite(&mut doc, &mut uses, type2(), empty_cmap.into(), &use_of(&[(65, 2)])), "CMap without CIDs"));
        let mut odd_map = type2();
        odd_map.set("CIDToGIDMap", 5);
        expect_none.push((composite(&mut doc, &mut uses, odd_map, name("Identity-H"), &one), "odd CIDToGIDMap"));
        let mut dct_map = type2();
        let m = doc.add_object(Stream::new(dictionary! { "Filter" => "DCTDecode" }, vec![0, 1]));
        dct_map.set("CIDToGIDMap", m);
        expect_none.push((composite(&mut doc, &mut uses, dct_map, name("Identity-H"), &one), "unreadable CIDToGIDMap"));
        expect_none.push((composite(&mut doc, &mut uses, type2(), name("Identity-H"), &wide), "CID beyond 16 bit"));
        expect_none.push((composite(&mut doc, &mut uses, dictionary! { "Type" => "Font", "Subtype" => "CIDFontType0" }, name("Identity-H"), &one), "nothing to gain"));
        // A CIDFontType0 over TrueType outlines.
        let (desc, file) = ttf(&mut doc);
        let cid = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "CIDFontType0", "FontDescriptor" => desc });
        let f = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0", "Encoding" => "Identity-H", "DescendantFonts" => vec![cid.into()] });
        uses.insert(f, one.clone());
        expect_none.push((file, "kind mismatch"));
        // A simple Type 1 font over TrueType outlines.
        let (desc, file) = ttf(&mut doc);
        let f = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "FontDescriptor" => desc });
        uses.insert(f, one.clone());
        expect_none.push((file, "Type1 with FontFile2"));
        // Not a font program at all, and one that cannot be decoded.
        for (stream, why) in [(Stream::new(Dictionary::new(), b"junk".to_vec()), "junk"), (Stream::new(dictionary! { "Filter" => "DCTDecode" }, font.clone()), "undecodable")] {
            let (desc, file) = descriptor(&mut doc, "J", "FontFile2", stream);
            let f = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "TrueType", "FontDescriptor" => desc });
            uses.insert(f, one.clone());
            expect_none.push((file, why));
        }

        let original = font_files(&doc);
        let files = collect(&doc, &uses, &original);
        for (file, why) in &expect_none {
            let info = files.get(file).expect(why);
            assert!(!info.unknown_use, "{why}");
            let before = doc.objects.get(file).unwrap().as_stream().unwrap().content.clone();
            assert_eq!(shrink_file(&mut doc, *file, info, &uses), None, "{why}");
            assert_eq!(doc.objects.get(file).unwrap().as_stream().unwrap().content, before, "{why}");
        }

        // Missing objects.
        let ghost = File { users: vec![User { font: (9001, 0), cid_font: None, descriptor: (9002, 0) }], unknown_use: false };
        assert_eq!(shrink_file(&mut doc, (9000, 0), &ghost, &uses), None);
        let program = Program::parse(&font).unwrap();
        assert!(used_glyphs(&doc, &ghost.users[0], &one, &program).is_none());
        let half = User { font: expect_none[0].0, cid_font: Some((9003, 0)), descriptor: (9002, 0) };
        let type0 = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0" });
        assert!(used_glyphs(&doc, &User { font: type0, ..half }, &one, &program).is_none());
        tag_name(&mut doc, (9004, 0), b"BaseFont", "AAAAAA");
        tag_name(&mut doc, type0, b"BaseFont", "AAAAAA");
        assert!(reduce(b"junk", &BTreeSet::new()).is_none());
        assert_eq!(subtype(&dictionary! { "Subtype" => 3 }), b"");
    }

    /// A page that shows "Formular" with the complete Roboto, embedded
    /// correctly. Returns the builder and the font dictionary.
    fn page_with_full_font() -> (B, ObjectId, ObjectId) {
        let font = roboto();
        let face = ttf_parser::Face::parse(&font, 0).unwrap();
        let mut widths = Vec::new();
        for c in 32..=126u8 {
            let advance = face.glyph_hor_advance(face.glyph_index(c as char).unwrap()).unwrap();
            widths.push(Object::Integer((advance as f64 * 1000.0 / face.units_per_em() as f64).round() as i64));
        }
        let mut b = B::new();
        let (desc, _) = descriptor(&mut b.doc, "Roboto-Regular", "FontFile2", Stream::new(dictionary! { "Length1" => font.len() as i64 }, font.clone()));
        let f = b.doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Roboto-Regular", "FirstChar" => 32, "LastChar" => 126, "Widths" => widths, "Encoding" => "WinAnsiEncoding", "FontDescriptor" => desc });
        b.page(b"BT /F1 12 Tf 72 700 Td (Formular) Tj ET", dictionary! { "Font" => dictionary! { "F1" => f } }, Dictionary::new());
        (b, f, desc)
    }

    #[test]
    fn whole_conversion_reduces_fonts_unless_told_not_to() {
        let (b, _, _) = page_with_full_font();
        let input = b.finish();
        let mut opts = crate::Options::new("2b".parse().unwrap());
        let small = crate::convert(&input, &opts).unwrap();
        assert!(small.report.has("fonts_reduced"), "{:?}", small.report.notes);
        opts.reduce_size = false;
        let large = crate::convert(&input, &opts).unwrap();
        assert!(!large.report.has("fonts_reduced"));
        assert!(small.bytes.len() * 3 < large.bytes.len(), "{} of {}", small.bytes.len(), large.bytes.len());
        // PDF/A-1: the new subset gets its name and is still a valid file.
        let p1 = crate::convert(&input, &crate::Options::new("1b".parse().unwrap())).unwrap();
        assert!(p1.report.has("fonts_reduced"));
        let doc = Document::load_mem(&p1.bytes).unwrap();
        let mut names = Vec::new();
        for o in doc.objects.values() {
            if let Ok(d) = o.as_dict() {
                if let Ok(n) = d.get(b"FontName").and_then(|n| n.as_name()) {
                    names.push(String::from_utf8_lossy(n).to_string());
                }
            }
        }
        assert!(names.len() == 1 && names[0].ends_with("+Roboto-Regular"), "{names:?}");
    }

    #[test]
    fn fonts_with_unknown_uses_are_left_alone() {
        // The form names the font for new input: a viewer may need any glyph.
        let (b, f, _) = page_with_full_font();
        let form = dictionary! { "Fields" => Vec::<Object>::new(), "DR" => dictionary! { "Font" => dictionary! { "F1" => f, "Direct" => dictionary! { "Type" => "Font" } } } };
        let with_form = b.finish_with(dictionary! { "AcroForm" => form });
        let out = crate::convert(&with_form, &crate::Options::new("2b".parse().unwrap())).unwrap();
        assert!(!out.report.has("fonts_reduced"), "{:?}", out.report.notes);

        // A second font dictionary on the same program that no text uses.
        let (mut b, _, desc) = page_with_full_font();
        b.doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Roboto-Regular", "Encoding" => "WinAnsiEncoding", "FontDescriptor" => desc });
        // Font dictionaries the pass does not follow: no descriptor, a
        // descriptor without a program, composite fonts without a CIDFont.
        let bare = b.doc.add_object(dictionary! { "Type" => "FontDescriptor" });
        b.doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "TrueType" });
        b.doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "TrueType", "FontDescriptor" => bare });
        b.doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "TrueType", "FontDescriptor" => Object::Reference((9100, 0)) });
        b.doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0" });
        b.doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0", "DescendantFonts" => vec![Object::Reference((9101, 0))] });
        b.doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type3" });
        let doc = b.doc;
        let uses = usage::collect(&doc);
        let files = collect(&doc, &uses, &font_files(&doc));
        assert_eq!(files.len(), 1);
        let info = files.values().next().unwrap();
        assert!(info.unknown_use && info.users.len() == 2);
        // Programs that were not part of the input are not looked at.
        assert!(collect(&doc, &uses, &BTreeSet::new()).is_empty());
        assert!(form_fonts(&doc).is_empty());
    }
}
