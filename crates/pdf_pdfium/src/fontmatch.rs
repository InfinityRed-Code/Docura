//! Finding the installed original of a font used in a PDF.
//!
//! Two uses:
//!
//! * **Recovering text.** PDFs printed through a PostScript / XPS driver
//!   (our own virtual printer included) embed TrueType fonts as bare glyph
//!   tables: no `cmap`, no `post`, no /ToUnicode — the content addresses
//!   glyphs by the *original font's* glyph ids, and PDFium reports those ids
//!   as if they were characters. The original is normally installed on this
//!   machine, so it can be found by fingerprint (same glyph count, same
//!   outlines for the glyphs in use), and its `cmap` turns glyph ids back
//!   into text.
//! * **Keeping the look.** Edited text is written with the installed
//!   original (embedded into the PDF) instead of a standard look-alike,
//!   found either by that fingerprint or by the font's PostScript name.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use ttf_parser::{name_id, Face, GlyphId};

/// An installed font face that can stand in for a PDF's font.
#[derive(Clone, Debug)]
pub(crate) struct FaceInfo {
    pub path: PathBuf,
    /// Face index inside a collection (0 for plain font files).
    pub index: u32,
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    pub monospaced: bool,
}

/// What a fingerprint-matched system font says about an embedded one.
pub(crate) struct Recovered {
    /// Glyph id → character.
    pub map: HashMap<u16, char>,
    pub face: FaceInfo,
}

/// One face of an installed font file.
struct Installed {
    path: PathBuf,
    index: u32,
    glyphs: u16,
    /// PostScript name, normalised (see `normalise`).
    ps_name: String,
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(at)?, *b.get(at + 1)?, *b.get(at + 2)?, *b.get(at + 3)?]))
}

/// Font names compare without case, spaces and the `,` / `-` style
/// separator variants ("Calibri,Bold" = "Calibri-Bold").
fn normalise(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != ',' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Glyph count and PostScript name of every face in a font file, reading
/// only the table directory, `maxp` and `name`.
fn face_headers(path: &Path) -> Option<Vec<(u16, String)>> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut read_at = |pos: u64, len: usize| -> Option<Vec<u8>> {
        let mut buf = vec![0u8; len];
        f.seek(SeekFrom::Start(pos)).ok()?;
        f.read_exact(&mut buf).ok()?;
        Some(buf)
    };
    let head = read_at(0, 12)?;
    let offsets: Vec<u32> = if &head[0..4] == b"ttcf" {
        let n = be32(&head, 8)?.min(64);
        let table = read_at(12, n as usize * 4)?;
        (0..n as usize).filter_map(|i| be32(&table, i * 4)).collect()
    } else {
        vec![0]
    };
    let mut faces = Vec::with_capacity(offsets.len());
    for off in offsets {
        let dir = read_at(off as u64, 12)?;
        let tables = be16(&dir, 4)?.min(128) as usize;
        let records = read_at(off as u64 + 12, tables * 16)?;
        let find = |tag: &[u8; 4]| {
            (0..tables)
                .find(|i| &records[i * 16..i * 16 + 4] == tag)
                .and_then(|i| Some((be32(&records, i * 16 + 8)?, be32(&records, i * 16 + 12)?)))
        };
        let (maxp_off, _) = find(b"maxp")?;
        let glyphs = be16(&read_at(maxp_off as u64, 6)?, 4)?;
        let ps_name = find(b"name")
            .filter(|(_, len)| *len <= 1 << 20)
            .and_then(|(o, len)| read_at(o as u64, len as usize))
            .and_then(|data| {
                let table = ttf_parser::name::Table::parse(&data)?;
                table
                    .names
                    .into_iter()
                    .filter(|n| n.name_id == name_id::POST_SCRIPT_NAME)
                    .find_map(|n| n.to_string())
            })
            .unwrap_or_default();
        faces.push((glyphs, normalise(&ps_name)));
    }
    Some(faces)
}

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(w) = std::env::var_os("WINDIR") {
        dirs.push(Path::new(&w).join("Fonts"));
    }
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(Path::new(&l).join("Microsoft").join("Windows").join("Fonts"));
    }
    if cfg!(not(windows)) {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(h) = std::env::var_os("HOME") {
            dirs.push(Path::new(&h).join(".fonts"));
            dirs.push(Path::new(&h).join(".local/share/fonts"));
        }
    }
    dirs
}

fn scan(dir: &Path, depth: u32, out: &mut Vec<Installed>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 4 {
                scan(&path, depth + 1, out);
            }
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if !matches!(ext.as_str(), "ttf" | "ttc" | "otf") {
            continue;
        }
        for (index, (glyphs, ps_name)) in
            face_headers(&path).unwrap_or_default().into_iter().enumerate()
        {
            out.push(Installed { path: path.clone(), index: index as u32, glyphs, ps_name });
        }
    }
}

/// Installed fonts, indexed once per process.
fn installed() -> &'static [Installed] {
    static INDEX: OnceLock<Vec<Installed>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let t = std::time::Instant::now();
        let mut out = Vec::new();
        for dir in font_dirs() {
            scan(&dir, 0, &mut out);
        }
        tracing::info!("fontmatch: indexed {} installed faces in {:?}", out.len(), t.elapsed());
        out
    })
}

/// True when `path` is one of the indexed installed font files — the only
/// files a text edit may embed.
pub(crate) fn is_installed(path: &Path) -> bool {
    installed().iter().any(|i| i.path == path)
}

fn face_info(inst: &Installed, face: &Face) -> FaceInfo {
    let family = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == name_id::FAMILY)
        .find_map(|n| n.to_string())
        .unwrap_or_default();
    FaceInfo {
        path: inst.path.clone(),
        index: inst.index,
        family,
        bold: face.is_bold(),
        italic: face.is_italic(),
        monospaced: face.is_monospaced(),
    }
}

/// PostScript name of the first face in a font file's bytes, as PDFium
/// names a font it embeds from them.
pub(crate) fn post_script_name(data: &[u8]) -> Option<String> {
    Face::parse(data, 0)
        .ok()?
        .names()
        .into_iter()
        .filter(|n| n.name_id == name_id::POST_SCRIPT_NAME)
        .find_map(|n| n.to_string())
}

/// The installed face with the PostScript name of a PDF font (subset prefix
/// "ABCDEF+" allowed). The metric twins of the standard fonts — Arial,
/// Times New Roman, Courier New — are left out: the standard fonts already
/// reproduce them without embedding anything.
pub(crate) fn by_name(pdf_font_name: &str) -> Option<Arc<FaceInfo>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<FaceInfo>>>>> = OnceLock::new();
    let base = pdf_font_name.split_once('+').map_or(pdf_font_name, |(_, n)| n);
    let key = normalise(base);
    if key.is_empty()
        || ["arial", "timesnewroman", "couriernew", "helvetica", "times", "courier"]
            .iter()
            .any(|std| key.starts_with(std))
    {
        return None;
    }
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().get(&key) {
        return hit.clone();
    }
    let found = installed().iter().filter(|i| i.ps_name == key).find_map(|inst| {
        let data = std::fs::read(&inst.path).ok()?;
        let face = Face::parse(&data, inst.index).ok()?;
        Some(Arc::new(face_info(inst, &face)))
    });
    cache.lock().insert(key, found.clone());
    found
}

/// Same advance and outline box for every glyph in use (and at least one
/// glyph that actually has an outline).
fn same_glyphs(embedded: &Face, candidate: &Face, used: &[u16]) -> bool {
    let mut outlined = 0;
    for &g in used {
        let id = GlyphId(g);
        let (a, b) = (embedded.glyph_bounding_box(id), candidate.glyph_bounding_box(id));
        if a != b {
            return false;
        }
        if a.is_some() {
            outlined += 1;
            if embedded.glyph_hor_advance(id) != candidate.glyph_hor_advance(id) {
                return false;
            }
        }
    }
    outlined > 0
}

fn invert_cmap(face: &Face) -> HashMap<u16, char> {
    let mut map: HashMap<u16, char> = HashMap::new();
    let Some(cmap) = face.tables().cmap else { return map };
    for subtable in cmap.subtables.into_iter().filter(|s| s.is_unicode()) {
        subtable.codepoints(|cp| {
            let (Some(c), Some(g)) = (char::from_u32(cp), subtable.glyph_index(cp)) else { return };
            if g.0 == 0 || c.is_control() {
                return;
            }
            // Several characters can share a glyph (space / no-break space,
            // hyphen / soft hyphen): keep the lowest, the plain one.
            map.entry(g.0).and_modify(|old| *old = (*old).min(c)).or_insert(c);
        });
    }
    map
}

fn recover_uncached(font_data: &[u8], used: &[u16]) -> Option<Recovered> {
    let embedded = Face::parse(font_data, 0).ok()?;
    let glyphs = embedded.number_of_glyphs();
    let upem = embedded.units_per_em();
    for inst in installed().iter().filter(|i| i.glyphs == glyphs) {
        let Ok(data) = std::fs::read(&inst.path) else { continue };
        let Ok(face) = Face::parse(&data, inst.index) else { continue };
        if face.units_per_em() != upem || !same_glyphs(&embedded, &face, used) {
            continue;
        }
        let map = invert_cmap(&face);
        if !used.iter().all(|g| map.contains_key(g)) {
            continue;
        }
        let face = face_info(inst, &face);
        tracing::info!(
            "fontmatch: embedded font matches {} #{} ({})",
            inst.path.display(),
            inst.index,
            face.family
        );
        return Some(Recovered { map, face });
    }
    None
}

/// Find the installed font an embedded, mapping-less TrueType font was cut
/// from. `used` are the glyph ids the text needs. Results (misses included)
/// are cached per font program and glyph set.
pub(crate) fn recover(font_data: &[u8], used: &[u16]) -> Option<Arc<Recovered>> {
    use std::hash::{Hash, Hasher};
    static CACHE: OnceLock<Mutex<HashMap<u64, Option<Arc<Recovered>>>>> = OnceLock::new();
    let mut used: Vec<u16> = used.to_vec();
    used.sort_unstable();
    used.dedup();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    font_data.hash(&mut h);
    used.hash(&mut h);
    let key = h.finish();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().get(&key) {
        return hit.clone();
    }
    let result = recover_uncached(font_data, &used).map(Arc::new);
    cache.lock().insert(key, result.clone());
    result
}
