//! Editing text that is part of the page content (as opposed to
//! annotations layered on top of it).
//!
//! PDFium exposes every text-showing operator of a page as a text page
//! object. `list` groups those objects into lines the UI can offer for
//! editing; `replace` removes a line's objects from a private copy of the
//! document, optionally paints over their box (scans: the visible glyphs
//! are pixels of the page image, only the OCR layer is text) and draws the
//! new text at the same baseline.
//!
//! The original font is not reused: embedded fonts are usually subsets that
//! lack the glyphs for new text. The replacement uses the closest standard
//! font instead (sans / serif / monospace, bold / italic); text outside
//! Latin-1 falls back to an embedded system TrueType font. A family the app
//! ships (`pdf_fonts`) is embedded in full.

use std::collections::HashMap;
use std::sync::Arc;

use pdf_types::{TextBlock, TextReplace};
use pdfium_render::prelude::*;

use crate::fontmatch::{self, FaceInfo};
use crate::{is_garbage_char, page_view, rect_to_top_origin, PageView};

/// One text page object, reduced to what line grouping needs. Coordinates
/// are PDF user space.
struct Piece {
    index: u32,
    text: String,
    left: f32,
    bottom: f32,
    right: f32,
    top: f32,
    /// Baseline start.
    ox: f32,
    oy: f32,
    /// Baseline direction in quarter turns counter-clockwise.
    q: u8,
    size: f32,
    family: &'static str,
    bold: bool,
    italic: bool,
    color: [f32; 3],
    invisible: bool,
    unreadable: bool,
    /// For unreadable text: the glyph ids PDFium reported in place of
    /// characters, and the font they belong to.
    glyphs: Vec<u32>,
    font_name: String,
    /// Installed original of the font, when it could be identified.
    face: Option<Arc<FaceInfo>>,
}

/// True when extracted text is evidently not what the glyphs show: fonts
/// without a Unicode mapping come out of PDFium as control codes, private
/// use characters or raw glyph ids that land in the phonetic blocks.
fn looks_unmapped(raw: &str) -> bool {
    let (mut total, mut odd) = (0u32, 0u32);
    for c in raw.chars().filter(|c| !c.is_whitespace() && *c != '\0') {
        total += 1;
        let cp = c as u32;
        if cp < 0x20
            || (0x7F..=0x9F).contains(&cp)
            || (0x0180..=0x02FF).contains(&cp)
            || (0xE000..=0xF8FF).contains(&cp)
            || (0xFFFD..=0xFFFF).contains(&cp)
        {
            odd += 1;
        }
    }
    total > 0 && odd * 4 >= total
}

impl Piece {
    /// Extent along the baseline direction.
    fn span(&self) -> (f32, f32) {
        match self.q {
            1 => (self.bottom, self.top),
            2 => (-self.right, -self.left),
            3 => (-self.top, -self.bottom),
            _ => (self.left, self.right),
        }
    }

    /// Position of the baseline across the reading direction.
    fn baseline(&self) -> f32 {
        match self.q {
            1 => -self.ox,
            2 => -self.oy,
            3 => self.ox,
            _ => self.oy,
        }
    }
}

/// Closest standard family for a font name ("Helv" | "Times" | "Cour").
fn family_for(name: &str, fixed_pitch: bool, serif: bool) -> &'static str {
    let name = name.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| name.contains(n));
    if has(&["courier", "mono", "consolas", "typewriter"]) || fixed_pitch {
        "Cour"
    } else if has(&["sans", "arial", "helvetica", "verdana", "tahoma", "calibri", "segoe"]) {
        "Helv"
    } else if has(&[
        "times", "serif", "georgia", "garamond", "cambria", "minion", "palatino", "bookman",
        "century",
    ]) || serif
    {
        "Times"
    } else {
        "Helv"
    }
}

/// Map a font to the closest standard family plus bold / italic.
fn classify_font(font: &PdfFont) -> (&'static str, bool, bool) {
    let name = font.name().to_ascii_lowercase();
    // Subset prefix "ABCDEF+".
    let name = name.split_once('+').map(|(_, n)| n.to_string()).unwrap_or(name);
    let has = |needles: &[&str]| needles.iter().any(|n| name.contains(n));
    let heavy = matches!(
        font.weight(),
        Ok(PdfFontWeight::Weight600
            | PdfFontWeight::Weight700Bold
            | PdfFontWeight::Weight800
            | PdfFontWeight::Weight900)
    );
    let bold = has(&["bold", "black", "heavy", "semibold", "demi"]) || heavy;
    let italic = has(&["italic", "oblique"]) || font.is_italic();
    (family_for(&name, font.is_fixed_pitch(), font.is_serif()), bold, italic)
}

/// Turn glyph ids back into text for pieces whose font has no Unicode
/// mapping, where the installed original of the font can be found (see
/// `fontmatch`). Pieces that cannot be recovered stay `unreadable`.
fn recover_unmapped(pieces: &mut [Piece], fonts: &HashMap<String, Vec<u8>>) {
    for (name, data) in fonts {
        let used: Vec<u16> = pieces
            .iter()
            .filter(|p| p.unreadable && &p.font_name == name)
            .flat_map(|p| p.glyphs.iter())
            .filter_map(|&g| u16::try_from(g).ok())
            .collect();
        let Some(found) = fontmatch::recover(data, &used) else { continue };
        let face = Arc::new(found.face.clone());
        for p in pieces.iter_mut().filter(|p| p.unreadable && &p.font_name == name) {
            let text: Option<String> = p
                .glyphs
                .iter()
                .map(|&g| u16::try_from(g).ok().and_then(|g| found.map.get(&g)).copied())
                .collect();
            let Some(text) = text else { continue };
            p.text = text;
            p.unreadable = false;
            p.family = family_for(&face.family, face.monospaced, false);
            p.bold = face.bold;
            p.italic = face.italic;
            p.face = Some(face.clone());
        }
    }
}

fn collect_pieces(page: &PdfPage) -> Result<Vec<Piece>, PdfiumError> {
    let text_page = page.text()?;
    let mut pieces = Vec::new();
    // Font programs of unreadable pieces, by font name.
    let mut unmapped_fonts: HashMap<String, Vec<u8>> = HashMap::new();
    for (index, object) in page.objects().iter().enumerate() {
        let Some(t) = object.as_text_object() else { continue };
        let raw = text_page.for_object(t);
        let unreadable = looks_unmapped(&raw);
        let text: String = raw
            .chars()
            .filter(|&c| c != '\0' && !is_garbage_char(c) && c != '\r' && c != '\n')
            .collect();
        if text.trim().is_empty() && !unreadable {
            continue;
        }
        let Ok(m) = object.matrix() else { continue };
        let (a, b, c, d) = (m.a(), m.b(), m.c(), m.d());
        let hlen = a.hypot(b);
        let vlen = c.hypot(d);
        // Mirrored, degenerate or not axis-aligned text is left alone.
        if hlen < 1e-4 || vlen < 1e-4 || a * d - b * c <= 0.0 {
            continue;
        }
        let (ux, uy) = (a / hlen, b / hlen);
        let q = if ux > 0.999 {
            0
        } else if uy > 0.999 {
            1
        } else if ux < -0.999 {
            2
        } else if uy < -0.999 {
            3
        } else {
            continue;
        };
        let Ok(bounds) = object.bounds() else { continue };
        let rect = bounds.to_rect();
        let size = t.unscaled_font_size().value * vlen;
        if size < 0.5 {
            continue;
        }
        let font = t.font();
        let (mut family, mut bold, mut italic) = classify_font(&font);
        // A font the app ships (left by an earlier edit or text box): edits
        // keep using it.
        let bundled = pdf_fonts::by_postscript_name(&font.name());
        if let Some((fam, b, i)) = bundled {
            (family, bold, italic) = (fam.id, b, i);
        }
        // An embedded font whose original is installed here: edits can keep
        // using the real thing.
        let face = if !unreadable && bundled.is_none() && font.is_embedded().unwrap_or(false) {
            fontmatch::by_name(&font.name())
        } else {
            None
        };
        if let Some(face) = &face {
            family = family_for(&face.family, face.monospaced, family == "Times");
            bold = face.bold;
            italic = face.italic;
        }
        let (mut glyphs, mut font_name) = (Vec::new(), String::new());
        if unreadable {
            // What PDFium hands out as characters are the glyph ids; the
            // spaces and breaks it makes up itself are not.
            if let Ok(chars) = t.chars(&text_page) {
                glyphs = chars
                    .iter()
                    .filter(|c| !c.is_generated().unwrap_or(false))
                    .map(|c| c.unicode_value())
                    .collect();
            }
            font_name = font.name();
            if !unmapped_fonts.contains_key(&font_name) {
                if let Ok(data) = font.data() {
                    unmapped_fonts.insert(font_name.clone(), data);
                }
            }
        }
        let color = object
            .fill_color()
            .map(|c| {
                [
                    c.red() as f32 / 255.0,
                    c.green() as f32 / 255.0,
                    c.blue() as f32 / 255.0,
                ]
            })
            .unwrap_or([0.0, 0.0, 0.0]);
        let invisible = matches!(
            t.render_mode(),
            PdfPageTextRenderMode::Invisible | PdfPageTextRenderMode::InvisibleClipping
        );
        pieces.push(Piece {
            index: index as u32,
            text,
            left: rect.left().value,
            bottom: rect.bottom().value,
            right: rect.right().value,
            top: rect.top().value,
            ox: m.e(),
            oy: m.f(),
            q,
            size,
            family,
            bold,
            italic,
            color,
            invisible,
            unreadable,
            glyphs,
            font_name,
            face,
        });
    }
    recover_unmapped(&mut pieces, &unmapped_fonts);
    pieces.retain(|p| p.unreadable || !p.text.trim().is_empty());
    Ok(pieces)
}

/// Group pieces into lines: same direction, same baseline, similar size and
/// no gap wider than a column gutter between neighbours.
fn group_lines(mut pieces: Vec<Piece>) -> Vec<Vec<Piece>> {
    pieces.sort_by(|x, y| {
        x.q.cmp(&y.q)
            .then(x.baseline().total_cmp(&y.baseline()))
            .then(x.span().0.total_cmp(&y.span().0))
    });
    // Rows: runs of pieces whose baselines sit within a fraction of the
    // font size of the row's first piece.
    let mut rows: Vec<Vec<Piece>> = Vec::new();
    for p in pieces {
        match rows.last_mut() {
            Some(row)
                if row[0].q == p.q
                    && (p.baseline() - row[0].baseline()).abs()
                        <= 0.3 * row[0].size.min(p.size) =>
            {
                row.push(p)
            }
            _ => rows.push(vec![p]),
        }
    }
    let mut lines: Vec<Vec<Piece>> = Vec::new();
    for mut row in rows {
        row.sort_by(|x, y| x.span().0.total_cmp(&y.span().0));
        let mut current: Vec<Piece> = Vec::new();
        let mut end = 0.0_f32;
        for p in row {
            let (start, stop) = p.span();
            let joins = current.last().is_some_and(|last| {
                let ratio = p.size / last.size;
                start - end < 1.2 * last.size.max(p.size)
                    && (0.7..=1.4).contains(&ratio)
                    && last.invisible == p.invisible
            });
            if joins {
                end = end.max(stop);
            } else {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }
                end = stop;
            }
            current.push(p);
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}

fn line_to_block(line: &[Piece], pv: &PageView) -> TextBlock {
    let first = &line[0];
    let mut text = String::new();
    let mut end = first.span().0;
    for p in line {
        let (start, stop) = p.span();
        if !text.is_empty()
            && start - end > 0.15 * p.size
            && !text.ends_with(char::is_whitespace)
            && !p.text.starts_with(char::is_whitespace)
        {
            text.push(' ');
        }
        text.push_str(&p.text);
        end = end.max(stop);
    }
    let fold = |f: fn(f32, f32) -> f32, get: fn(&Piece) -> f32| {
        line.iter().map(get).fold(get(first), f)
    };
    let rect = PdfRect::new_from_values(
        fold(f32::min, |p| p.bottom),
        fold(f32::min, |p| p.left),
        fold(f32::max, |p| p.top),
        fold(f32::max, |p| p.right),
    );
    let origin = rect_to_top_origin(
        PdfRect::new_from_values(first.oy, first.ox, first.oy, first.ox),
        pv,
    );
    let unreadable = line.iter().any(|p| p.unreadable);
    // PDFium embeds the first face of a font file only.
    let face = first.face.as_deref().filter(|f| f.index == 0);
    TextBlock {
        objects: line.iter().map(|p| p.index).collect(),
        text: if unreadable { String::new() } else { text.trim_end().to_string() },
        unreadable,
        font_file: face.map(|f| f.path.to_string_lossy().into_owned()),
        font_name: face.map(|f| f.family.clone()),
        rect_pt: rect_to_top_origin(rect, pv),
        origin_x: origin.x0,
        origin_y: origin.y0,
        // Counter-clockwise in PDF space → clockwise on screen, plus /Rotate.
        dir: ((4 - first.q as i32) % 4 + pv.rot / 90).rem_euclid(4) as u8,
        font_size: first.size,
        font_family: first.family.to_string(),
        bold: first.bold,
        italic: first.italic,
        color: first.color,
        invisible: first.invisible,
    }
}

pub(crate) fn list(page: &PdfPage) -> Result<Vec<TextBlock>, PdfiumError> {
    let pv = page_view(page);
    let lines = group_lines(collect_pieces(page)?);
    Ok(lines.iter().map(|l| line_to_block(l, &pv)).collect())
}

/// True when the standard fonts can show `c` *and* PDFium reads it back as
/// the same character. That holds for Latin-1; the WinAnsi extras (the euro
/// sign, typographic quotes, dashes) render but extract as C1 control codes,
/// so they take the embedded-font path like everything else.
fn is_std_font_char(c: char) -> bool {
    let cp = c as u32;
    (0x20..=0x7E).contains(&cp) || (0xA0..=0xFF).contains(&cp)
}

/// A system TrueType font file for text the standard fonts cannot carry.
fn system_font_bytes(family: &str, bold: bool, italic: bool) -> Option<Vec<u8>> {
    let stem = match family {
        "Times" => "times",
        "Cour" => "cour",
        _ => "arial",
    };
    let suffix = match (bold, italic) {
        (true, true) => "bi",
        (true, false) => "bd",
        (false, true) => "i",
        (false, false) => "",
    };
    let windir = std::env::var_os("WINDIR")?;
    let dir = std::path::Path::new(&windir).join("Fonts");
    std::fs::read(dir.join(format!("{stem}{suffix}.ttf")))
        .or_else(|_| std::fs::read(dir.join("arial.ttf")))
        .ok()
}

fn builtin_font(family: &str, bold: bool, italic: bool) -> PdfFontBuiltin {
    match (family, bold, italic) {
        ("Times", false, false) => PdfFontBuiltin::TimesRoman,
        ("Times", true, false) => PdfFontBuiltin::TimesBold,
        ("Times", false, true) => PdfFontBuiltin::TimesItalic,
        ("Times", true, true) => PdfFontBuiltin::TimesBoldItalic,
        ("Cour", false, false) => PdfFontBuiltin::Courier,
        ("Cour", true, false) => PdfFontBuiltin::CourierBold,
        ("Cour", false, true) => PdfFontBuiltin::CourierOblique,
        ("Cour", true, true) => PdfFontBuiltin::CourierBoldOblique,
        (_, false, false) => PdfFontBuiltin::Helvetica,
        (_, true, false) => PdfFontBuiltin::HelveticaBold,
        (_, false, true) => PdfFontBuiltin::HelveticaOblique,
        (_, true, true) => PdfFontBuiltin::HelveticaBoldOblique,
    }
}

fn to_color(rgb: [f32; 3]) -> PdfColor {
    let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    PdfColor::new(ch(rgb[0]), ch(rgb[1]), ch(rgb[2]), 255)
}

const STALE: &str = "replace text: the page changed, please try again";

/// Where the font for the new text comes from.
enum FontSource {
    Builtin(PdfFontBuiltin),
    /// A font file to embed (or to reuse, if the page already carries it).
    File(Vec<u8>),
}

fn font_source(edit: &TextReplace, text: &str) -> Result<FontSource, String> {
    if let Some(file) = &edit.font_file {
        let path = std::path::Path::new(file);
        if !fontmatch::is_installed(path) {
            return Err("replace text: not an installed font file".into());
        }
        return std::fs::read(path)
            .map(FontSource::File)
            .map_err(|e| format!("replace text: read font: {e}"));
    }
    if let Some(family) = pdf_fonts::family(&edit.font_family) {
        return Ok(FontSource::File(family.face(edit.bold, edit.italic).to_vec()));
    }
    if text.chars().all(|c| c == '\n' || is_std_font_char(c)) {
        return Ok(FontSource::Builtin(builtin_font(&edit.font_family, edit.bold, edit.italic)));
    }
    system_font_bytes(&edit.font_family, edit.bold, edit.italic)
        .map(FontSource::File)
        .ok_or_else(|| "replace text: no font available for these characters".into())
}

/// A font this page already embeds in full under `ps_name` — what an earlier
/// edit left behind — so the same font file is not embedded once per edit.
/// Fonts only used by the objects about to be removed do not count.
fn reusable_font(page: &PdfPage, ps_name: &str, removed: &[u32]) -> Option<PdfFontToken> {
    page.objects().iter().enumerate().find_map(|(i, object)| {
        if removed.contains(&(i as u32)) {
            return None;
        }
        let font = object.as_text_object()?.font();
        (font.name() == ps_name && font.is_embedded().unwrap_or(false)).then(|| font.token())
    })
}

/// Append `lines` at the anchor; returns how many objects were added.
fn add_lines<'a>(
    document: &PdfDocument<'a>,
    page: &mut PdfPage<'a>,
    font: PdfFontToken,
    edit: &TextReplace,
    lines: &[&str],
    (ox, oy, ux, uy): (f32, f32, f32, f32),
) -> Result<u32, String> {
    let err = |what: &str, e: PdfiumError| format!("{what}: {e}");
    let size = edit.font_size.clamp(1.0, 2000.0);
    let leading = size * 1.2;
    let mut added = 0;
    for (n, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut object = PdfPageTextObject::new(document, *line, font, PdfPoints::new(size))
            .map_err(|e| err("create text", e))?;
        object
            .set_fill_color(to_color(edit.color))
            .map_err(|e| err("text color", e))?;
        // Further lines stack below the first, across the baseline.
        let down = n as f32 * leading;
        object
            .transform(ux, uy, -uy, ux, ox + uy * down, oy - ux * down)
            .map_err(|e| err("place text", e))?;
        page.objects_mut()
            .add_text_object(object)
            .map_err(|e| err("add text", e))?;
        added += 1;
    }
    page.regenerate_content().map_err(|e| err("regenerate", e))?;
    Ok(added)
}

/// Detach and destroy page objects by index. Raw bindings: pdfium-render's
/// `remove_object` hands back a wrapper that destroys a detached text object
/// twice when dropped. Handles are collected first — removing shifts the
/// indices.
fn remove_objects(page: &PdfPage, indices: impl Iterator<Item = u32>) -> Result<(), String> {
    let bindings = page.bindings();
    let page_handle = bindings.get_handle_from_page(page);
    let handles: Vec<FPDF_PAGEOBJECT> = indices
        .map(|i| bindings.FPDFPage_GetObject(page_handle, i as std::os::raw::c_int))
        .collect();
    for handle in handles {
        if handle.is_null()
            || !bindings.is_true(bindings.FPDFPage_RemoveObject(page_handle, handle))
        {
            return Err("replace text: could not remove the old text".into());
        }
        bindings.FPDFPageObj_Destroy(handle);
    }
    Ok(())
}

/// True when the last `added` objects of the page read back as `lines` — a
/// reused font is only trusted if it maps the new text both ways.
fn reads_back(page: &PdfPage, added: u32, lines: &[&str]) -> bool {
    let Ok(text_page) = page.text() else { return false };
    let count = page.objects().len() as u32;
    let wanted = lines.iter().filter(|l| !l.trim().is_empty());
    (count - added..count).zip(wanted).all(|(i, line)| {
        page.objects().get(i as usize).ok().is_some_and(|object| {
            object.as_text_object().is_some_and(|t| {
                let got: String = text_page.for_object(t).chars().filter(|&c| c != '\0').collect();
                got.trim() == line.trim()
            })
        })
    })
}

fn open_page<'a>(document: &PdfDocument<'a>, index: u16) -> Result<PdfPage<'a>, String> {
    let mut page = document.pages().get(index).map_err(|e| format!("page: {e}"))?;
    // One content rewrite per step instead of one per touched object.
    page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
    Ok(page)
}

/// Apply `edit` to page `page_index` of `document` (a private copy).
pub(crate) fn replace(
    document: &mut PdfDocument,
    page_index: u16,
    edit: &TextReplace,
) -> Result<(), String> {
    let err = |what: &str, e: PdfiumError| format!("{what}: {e}");
    if edit.objects.is_empty() {
        return Err("replace text: no objects".into());
    }
    let text: String = edit.text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.split('\n').map(str::trim_end).collect();
    let has_text = lines.iter().any(|l| !l.trim().is_empty());
    let source = if has_text { Some(font_source(edit, &text)?) } else { None };

    let mut indices: Vec<u32> = edit.objects.clone();
    indices.sort_unstable();
    indices.dedup();

    // Baseline, direction and box of the text being replaced; then remove
    // it and paint the cover.
    let (anchor, reused) = {
        let mut page = open_page(document, page_index)?;
        let count = page.objects().len() as u32;
        let mut anchor: Option<(f32, f32, f32, f32)> = None; // ox, oy, ux, uy
        let mut bbox: Option<(f32, f32, f32, f32)> = None; // l, b, r, t
        for &i in &indices {
            if i >= count {
                return Err(STALE.into());
            }
            let object = page.objects().get(i as usize).map_err(|e| err("object", e))?;
            if object.as_text_object().is_none() {
                return Err(STALE.into());
            }
            let m = object.matrix().map_err(|e| err("matrix", e))?;
            if i == edit.objects[0] {
                let len = m.a().hypot(m.b()).max(1e-6);
                anchor = Some((m.e(), m.f(), m.a() / len, m.b() / len));
            }
            if let Ok(b) = object.bounds() {
                let r = b.to_rect();
                let (l, bo, ri, t) =
                    (r.left().value, r.bottom().value, r.right().value, r.top().value);
                bbox = Some(match bbox {
                    Some((bl, bb, br, bt)) => (bl.min(l), bb.min(bo), br.max(ri), bt.max(t)),
                    None => (l, bo, ri, t),
                });
            }
        }
        let anchor = anchor.ok_or("replace text: no anchor object")?;
        let reused = match &source {
            Some(FontSource::File(bytes)) => fontmatch::post_script_name(bytes)
                .and_then(|name| reusable_font(&page, &name, &indices)),
            _ => None,
        };

        remove_objects(&page, indices.iter().copied())?;
        if let (Some(cover), Some((l, b, r, t))) = (edit.cover, bbox) {
            let pad = (edit.font_size * 0.12).max(0.5);
            page.objects_mut()
                .create_path_object_rect(
                    PdfRect::new_from_values(b - pad, l - pad, t + pad, r + pad),
                    None,
                    None,
                    Some(to_color(cover)),
                )
                .map_err(|e| err("cover", e))?;
        }
        page.regenerate_content().map_err(|e| err("regenerate", e))?;

        // First try with a font the page already embeds.
        let mut kept = None;
        if let Some(font) = reused {
            let added = add_lines(document, &mut page, font, edit, &lines, anchor)?;
            if reads_back(&page, added, &lines) {
                kept = Some(font);
            } else {
                let count = page.objects().len() as u32;
                remove_objects(&page, count - added..count)?;
                page.regenerate_content().map_err(|e| err("regenerate", e))?;
            }
        }
        (anchor, kept)
    };

    let Some(source) = source else { return Ok(()) };
    if reused.is_some() {
        return Ok(());
    }
    // Loading a font needs the document mutably, hence the page was let go.
    let font = match source {
        FontSource::Builtin(builtin) => document.fonts_mut().new_built_in(builtin),
        FontSource::File(bytes) => document
            .fonts_mut()
            .load_true_type_from_bytes(&bytes, true)
            .map_err(|e| err("load font", e))?,
    };
    let mut page = open_page(document, page_index)?;
    add_lines(document, &mut page, font, edit, &lines, anchor)?;
    Ok(())
}
