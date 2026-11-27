//! Appearance streams of text and choice fields.
//!
//! A filled-in field is drawn by its widget's /AP /N stream, not by /V, so
//! writing a value means writing that stream too: with the font size of the
//! field's /DA (or a size chosen to fit, for "auto" = 0), its colour, its
//! alignment /Q, its border and background /MK and its own rotation /MK /R.
//! The same layout numbers are reported to the UI (`text_styles`) so the
//! editor placed over a field sets its text exactly as the page shows it.

use std::collections::HashMap;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use pdf_types::FieldTextStyle;

use crate::std_metrics::StdWidths;
use crate::{MutateError, MutateResult};

/// Bound for /Parent chain walks.
const MAX_DEPTH: usize = 64;
/// Smallest size automatic sizing shrinks a text to.
pub const MIN_AUTO_SIZE: f32 = 4.0;
/// Size an auto-sized multi-line field or list starts from.
pub const MULTILINE_AUTO_SIZE: f32 = 12.0;
/// Distance between baselines in font sizes (the editor's line-height).
pub const LEADING: f32 = 1.15;
/// First baseline below the top of a line box, in font sizes.
const ASCENT: f32 = 0.92;
/// Height of capitals in font sizes: what a single line is centred on.
const CAP_HEIGHT: f32 = 0.718;

const FF_MULTILINE: u32 = 1 << 12;
const FF_PASSWORD: u32 = 1 << 13;
const FF_COMBO: u32 = 1 << 17;
const FF_COMB: u32 = 1 << 24;

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(r) => doc.get_object(*r).unwrap_or(o),
        _ => o,
    }
}

fn get<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    d.get(key).ok().map(|o| deref(doc, o))
}

fn dict<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Dictionary> {
    get(doc, d, key)?.as_dict().ok()
}

fn number(o: &Object) -> Option<f32> {
    match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

fn numbers(doc: &Document, o: &Object) -> Vec<f32> {
    match deref(doc, o) {
        Object::Array(a) => a.iter().filter_map(|v| number(deref(doc, v))).collect(),
        _ => Vec::new(),
    }
}

fn string(o: &Object) -> Option<String> {
    o.as_str().ok().map(crate::read::decode_pdf_string)
}

/// A field attribute, looked up along the /Parent chain.
fn inherited<'a>(doc: &'a Document, widget: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    let mut cur = widget;
    for _ in 0..MAX_DEPTH {
        if let Some(v) = get(doc, cur, key) {
            return Some(v);
        }
        cur = dict(doc, cur, b"Parent")?;
    }
    None
}

fn acroform(doc: &Document) -> Option<&Dictionary> {
    let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    dict(doc, doc.get_dictionary(root).ok()?, b"AcroForm")
}

/// A default appearance string: `/Helv 12 Tf 0 g`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Da {
    /// Resource name of the font, without the slash.
    pub font: String,
    /// Font size; 0 = automatic.
    pub size: f32,
    /// The colour operator with its operands, as written.
    pub color_ops: String,
    pub rgb: [f32; 3],
}

impl Default for Da {
    fn default() -> Self {
        Da { font: "Helv".into(), size: 0.0, color_ops: "0 g".into(), rgb: [0.0; 3] }
    }
}

pub(crate) fn parse_da(da: &str) -> Da {
    let mut out = Da::default();
    let tokens: Vec<&str> = da.split_ascii_whitespace().collect();
    let nums = |i: usize, n: usize| -> Option<Vec<f32>> {
        if i < n {
            return None;
        }
        tokens[i - n..i].iter().map(|t| t.parse::<f32>().ok()).collect()
    };
    for (i, t) in tokens.iter().enumerate() {
        match *t {
            "Tf" if i >= 2 => {
                out.font = tokens[i - 2].trim_start_matches('/').to_string();
                out.size = tokens[i - 1].parse::<f32>().unwrap_or(0.0).max(0.0);
            }
            "g" => {
                if let Some(v) = nums(i, 1) {
                    out.rgb = [v[0]; 3];
                    out.color_ops = format!("{} g", tokens[i - 1]);
                }
            }
            "rg" => {
                if let Some(v) = nums(i, 3) {
                    out.rgb = [v[0], v[1], v[2]];
                    out.color_ops = format!("{} rg", tokens[i - 3..i].join(" "));
                }
            }
            "k" => {
                if let Some(v) = nums(i, 4) {
                    let f = |c: f32| (1.0 - c.clamp(0.0, 1.0)) * (1.0 - v[3].clamp(0.0, 1.0));
                    out.rgb = [f(v[0]), f(v[1]), f(v[2])];
                    out.color_ops = format!("{} k", tokens[i - 4..i].join(" "));
                }
            }
            _ => {}
        }
    }
    out
}

/// The closest standard family and style for a font name ("ArialMT",
/// "ABCDEF+TimesNewRomanPS-BoldMT", or a bare resource name like "HeBo").
pub(crate) fn classify_font(name: &str) -> (&'static str, bool, bool) {
    match name {
        "Helv" => return ("Helv", false, false),
        "HeBo" => return ("Helv", true, false),
        "HeOb" => return ("Helv", false, true),
        "HeBO" => return ("Helv", true, true),
        "TiRo" => return ("Times", false, false),
        "TiBo" => return ("Times", true, false),
        "TiIt" => return ("Times", false, true),
        "TiBI" => return ("Times", true, true),
        "Cour" => return ("Cour", false, false),
        "CoBo" => return ("Cour", true, false),
        "CoOb" => return ("Cour", false, true),
        "CoBO" => return ("Cour", true, true),
        _ => {}
    }
    let n = name.split_once('+').map_or(name, |(_, n)| n).to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    let family = if has(&["cour", "mono", "consol"]) {
        "Cour"
    } else if !n.contains("sans") && has(&["times", "serif", "georgia", "garamond", "minion", "palatino", "cambria"]) {
        "Times"
    } else {
        "Helv"
    };
    (family, has(&["bold", "black", "heavy"]), has(&["italic", "oblique"]))
}

fn winansi(c: char) -> Option<u8> {
    let b = crate::annotations::winansi_byte(c);
    (b != b'?' || c == '?').then_some(b)
}

/// The font an appearance is written with, and how to measure it.
enum Face {
    /// A simple font with WinAnsiEncoding: the field's own font or a
    /// standard one. `own` holds /FirstChar and /Widths if the font has them.
    WinAnsi { std: StdWidths, own: Option<(u32, Vec<f32>)> },
    /// A bundled TrueType face for text outside WinAnsi; strings are glyph ids.
    Embedded(Box<ttf_parser::Face<'static>>),
}

/// Where the font object comes from.
enum Source {
    /// The font the field names in /DA, from the form's /DR.
    Field(ObjectId),
    /// A standard font (family, bold, italic).
    Standard(&'static str, bool, bool),
    /// A bundled family.
    Bundled(&'static pdf_fonts::Family, bool, bool),
}

struct FieldFont {
    source: Source,
    face: Face,
    family: &'static str,
    bold: bool,
    italic: bool,
}

impl FieldFont {
    /// Width of `text` at font size 1.
    fn width(&self, text: &str) -> f32 {
        match &self.face {
            Face::WinAnsi { std, own } => text
                .chars()
                .map(|c| {
                    let b = winansi(c).unwrap_or(b'?');
                    own.as_ref()
                        .and_then(|(first, w)| w.get((b as u32).checked_sub(*first)? as usize).copied())
                        .filter(|w| *w > 0.0)
                        .unwrap_or(std.width(b) as f32)
                })
                .sum::<f32>()
                / 1000.0,
            Face::Embedded(face) => {
                let upm = face.units_per_em() as f32;
                text.chars()
                    .map(|c| face.glyph_hor_advance(crate::truetype::glyph(face, c)).unwrap_or(0) as f32)
                    .sum::<f32>()
                    / upm
            }
        }
    }

    /// `text` as a string operand for `Tj`.
    fn encode(&self, text: &str) -> Vec<u8> {
        match &self.face {
            Face::WinAnsi { .. } => crate::annotations::pdf_literal_string(text),
            Face::Embedded(face) => crate::truetype::encode(face, text),
        }
    }
}

/// Picks the font for `text` in a field whose /DA names `da_font`.
fn field_font(doc: &Document, da_font: &str, text: &str) -> FieldFont {
    let dr_font = acroform(doc)
        .and_then(|af| dict(doc, af, b"DR"))
        .and_then(|dr| dict(doc, dr, b"Font"))
        .and_then(|fonts| match fonts.get(da_font.as_bytes()).ok()? {
            Object::Reference(id) => Some((*id, doc.get_dictionary(*id).ok()?)),
            _ => None,
        });
    let base = dr_font
        .and_then(|(_, d)| get(doc, d, b"BaseFont"))
        .and_then(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned());
    let (family, bold, italic) = classify_font(base.as_deref().unwrap_or(da_font));

    if !text.chars().all(|c| winansi(c).is_some()) {
        // Text the standard encodings cannot write: a bundled face that
        // matches the family, embedded with the glyphs it needs.
        let want = match family {
            "Times" => pdf_fonts::Category::Serif,
            "Cour" => pdf_fonts::Category::Mono,
            _ => pdf_fonts::Category::Sans,
        };
        let fams = pdf_fonts::families();
        if let Some(fam) = fams.iter().find(|f| f.category == want).or(fams.first()) {
            let (b, i) = fam.effective_style(bold, italic);
            if let Ok(face) = crate::truetype::parse(fam.face(b, i)) {
                return FieldFont { source: Source::Bundled(fam, b, i), face: Face::Embedded(Box::new(face)), family, bold, italic };
            }
        }
    }

    let std = StdWidths::of(family, bold, italic);
    if let Some((id, d)) = dr_font {
        let simple = matches!(
            get(doc, d, b"Subtype").and_then(|o| o.as_name().ok()),
            Some(b"Type1") | Some(b"TrueType") | Some(b"MMType1")
        );
        let win_ansi = get(doc, d, b"Encoding").and_then(|o| o.as_name().ok()) == Some(b"WinAnsiEncoding".as_slice());
        if simple && win_ansi {
            let own = get(doc, d, b"FirstChar").and_then(number).and_then(|first| {
                let w = numbers(doc, d.get(b"Widths").ok()?);
                (!w.is_empty()).then_some((first.max(0.0) as u32, w))
            });
            return FieldFont { source: Source::Field(id), face: Face::WinAnsi { std, own }, family, bold, italic };
        }
    }
    FieldFont { source: Source::Standard(family, bold, italic), face: Face::WinAnsi { std, own: None }, family, bold, italic }
}

/// Resource name and object of the font, creating it where needed.
fn materialize(doc: &mut Document, da_font: &str, font: &FieldFont) -> MutateResult<(String, ObjectId)> {
    match &font.source {
        Source::Field(id) => Ok((da_font.to_string(), *id)),
        Source::Standard(family, bold, italic) => {
            let (key, base) = crate::annotations::std_font(family, *bold, *italic);
            // A name of our own: the form's /Helv may be a font with another
            // encoding.
            let key = format!("Dc{key}");
            let id = crate::annotations::ensure_std_font(doc, &key, base)?;
            Ok((key, id))
        }
        Source::Bundled(fam, bold, italic) => {
            let (key, id, _) = crate::annotations::freetext_font(doc, fam.id, *bold, *italic)?;
            Ok((key, id))
        }
    }
}

/// Everything about a text or choice widget its appearance depends on.
struct Widget {
    rect: [f32; 4],
    /// /MK /R in quarter turns counter-clockwise.
    mk_turns: u8,
    flags: u32,
    choice: bool,
    da: Da,
    q: u8,
    /// The text shown (display value of a choice field).
    value: String,
    /// List box: the options and the selected ones.
    options: Vec<String>,
    selected: Vec<usize>,
    max_len: Option<usize>,
    border: f32,
    underline: bool,
    dash: Option<Vec<f32>>,
    border_color: Vec<f32>,
    background: Vec<f32>,
}

impl Widget {
    /// Size of the box in the orientation of its text.
    fn text_box(&self) -> (f32, f32) {
        let (w, h) = (self.rect[2] - self.rect[0], self.rect[3] - self.rect[1]);
        if self.mk_turns % 2 == 1 { (h, w) } else { (w, h) }
    }

    fn multiline(&self) -> bool {
        !self.choice && self.flags & FF_MULTILINE != 0
    }

    fn list(&self) -> bool {
        self.choice && self.flags & FF_COMBO == 0
    }
}

/// Text inset from the widget's edge: twice the border, at least 2.
pub fn text_padding(border: f32) -> f32 {
    (2.0 * border).max(2.0)
}

fn read_widget(doc: &Document, w: &Dictionary) -> Option<Widget> {
    let ft = inherited(doc, w, b"FT")?.as_name().ok()?;
    let choice = match ft {
        b"Tx" => false,
        b"Ch" => true,
        _ => return None,
    };
    let r = numbers(doc, w.get(b"Rect").ok()?);
    if r.len() != 4 {
        return None;
    }
    let rect = [r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])];
    if rect[2] - rect[0] <= 0.0 || rect[3] - rect[1] <= 0.0 {
        return None;
    }
    let flags = inherited(doc, w, b"Ff").and_then(number).unwrap_or(0.0) as u32;
    let da = inherited(doc, w, b"DA")
        .and_then(string)
        .or_else(|| acroform(doc).and_then(|af| get(doc, af, b"DA")).and_then(string))
        .map(|s| parse_da(&s))
        .unwrap_or_default();
    let q = inherited(doc, w, b"Q")
        .and_then(number)
        .or_else(|| acroform(doc).and_then(|af| get(doc, af, b"Q")).and_then(number))
        .unwrap_or(0.0) as i64;

    // Options: a string, or [export value, display text].
    let mut options: Vec<(String, String)> = Vec::new();
    if choice {
        if let Some(Object::Array(opts)) = inherited(doc, w, b"Opt") {
            for o in opts {
                match deref(doc, o) {
                    Object::Array(pair) if pair.len() >= 2 => {
                        let e = string(deref(doc, &pair[0])).unwrap_or_default();
                        let d = string(deref(doc, &pair[1])).unwrap_or_default();
                        options.push((e, d));
                    }
                    other => {
                        let s = string(other).unwrap_or_default();
                        options.push((s.clone(), s));
                    }
                }
            }
        }
    }
    let values: Vec<String> = match inherited(doc, w, b"V") {
        Some(Object::Array(a)) => a.iter().filter_map(|v| string(deref(doc, v))).collect(),
        Some(o) => string(o).into_iter().collect(),
        None => Vec::new(),
    };
    let display = |v: &String| options.iter().find(|(e, _)| e == v).map_or(v.clone(), |(_, d)| d.clone());
    let value = values.iter().map(display).collect::<Vec<_>>().join(", ");
    let selected = options
        .iter()
        .enumerate()
        .filter(|(_, (e, _))| values.contains(e))
        .map(|(i, _)| i)
        .collect();

    let mk = dict(doc, w, b"MK");
    let color = |key: &[u8]| mk.and_then(|m| m.get(key).ok()).map(|o| numbers(doc, o)).unwrap_or_default();
    let border_color = color(b"BC");
    let bs = dict(doc, w, b"BS");
    let border = bs
        .and_then(|b| get(doc, b, b"W"))
        .and_then(number)
        .or_else(|| numbers(doc, w.get(b"Border").ok()?).get(2).copied())
        .unwrap_or(if border_color.is_empty() { 0.0 } else { 1.0 })
        .max(0.0);
    let style = bs.and_then(|b| get(doc, b, b"S")).and_then(|o| o.as_name().ok());
    let dash = (style == Some(b"D".as_slice()))
        .then(|| bs.and_then(|b| b.get(b"D").ok()).map(|o| numbers(doc, o)).filter(|d| !d.is_empty()).unwrap_or(vec![3.0]));
    let r = mk.and_then(|m| get(doc, m, b"R")).and_then(number).unwrap_or(0.0) as i64;

    Some(Widget {
        rect,
        mk_turns: (r.rem_euclid(360) / 90) as u8,
        flags,
        choice,
        da,
        q: q.clamp(0, 2) as u8,
        value,
        options: options.into_iter().map(|(_, d)| d).collect(),
        selected,
        max_len: inherited(doc, w, b"MaxLen").and_then(number).map(|n| n.max(0.0) as usize),
        border,
        underline: style == Some(b"U".as_slice()),
        dash,
        border_color,
        background: color(b"BG"),
    })
}

/// Largest size at which one line of `width_at_1` (its width at size 1)
/// fills a box `h` high and stays within `avail_w`.
pub fn auto_size_single(h: f32, border: f32, avail_w: f32, width_at_1: f32) -> f32 {
    let by_height = (h - 2.0 * border - 2.0) / LEADING;
    let by_width = if width_at_1 > 0.0 { avail_w / width_at_1 } else { f32::INFINITY };
    ((by_height.min(by_width) * 100.0).floor() / 100.0).max(MIN_AUTO_SIZE)
}

/// Largest size (from `MULTILINE_AUTO_SIZE` down in half points) at which
/// `lines(size)` lines fit into `avail_h`.
pub fn auto_size_lines(avail_h: f32, lines: impl Fn(f32) -> usize) -> f32 {
    let mut size = MULTILINE_AUTO_SIZE;
    while size > MIN_AUTO_SIZE && lines(size) as f32 * size * LEADING > avail_h {
        size -= 0.5;
    }
    size
}

/// Breaks `text` into lines no wider than `avail` (`measure` = width at the
/// font size in use). Explicit line breaks are kept; a word wider than a
/// line is split.
pub fn wrap_text(text: &str, avail: f32, measure: impl Fn(&str) -> f32) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if measure(&candidate) <= avail || line.is_empty() && measure(word) <= avail {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            // A single word that does not fit: split it by characters.
            for c in word.chars() {
                let mut next = line.clone();
                next.push(c);
                if measure(&next) > avail && !line.is_empty() {
                    out.push(std::mem::replace(&mut line, c.to_string()));
                } else {
                    line = next;
                }
            }
        }
        out.push(line);
    }
    out
}

/// The text of a widget as laid out in its box.
struct Layout {
    size: f32,
    /// (x, baseline y, text) in the box's own bottom-left coordinates.
    runs: Vec<(f32, f32, String)>,
    /// Highlighted rows of a list box: (y, height).
    marks: Vec<(f32, f32)>,
}

fn layout(w: &Widget, font: &FieldFont) -> Layout {
    let (bw, bh) = w.text_box();
    let pad = text_padding(w.border);
    let avail_w = (bw - 2.0 * pad).max(1.0);
    let avail_h = (bh - 2.0 * pad).max(1.0);
    let align = |size: f32, text: &str| match w.q {
        1 => (bw - font.width(text) * size) / 2.0,
        2 => bw - pad - font.width(text) * size,
        _ => pad,
    };
    let mut marks = Vec::new();

    if w.multiline() || w.list() {
        let lines_at = |size: f32| -> Vec<String> {
            if w.list() {
                w.options.clone()
            } else {
                wrap_text(&w.value, avail_w, |s| font.width(s) * size)
            }
        };
        let size = if w.da.size > 0.0 { w.da.size } else { auto_size_lines(avail_h, |s| lines_at(s).len()) };
        let lead = size * LEADING;
        let runs = lines_at(size)
            .into_iter()
            .enumerate()
            .map(|(i, line)| {
                let top = bh - pad - i as f32 * lead;
                if w.list() && w.selected.contains(&i) {
                    marks.push((top - lead, lead));
                }
                (align(size, &line), top - ASCENT * size, line)
            })
            .collect();
        return Layout { size, runs, marks };
    }

    let text: String = if w.flags & FF_PASSWORD != 0 && !w.choice {
        "*".repeat(w.value.chars().count())
    } else {
        w.value.replace(['\r', '\n'], " ")
    };
    let size = if w.da.size > 0.0 { w.da.size } else { auto_size_single(bh, w.border, avail_w, font.width(&text)) };
    let y = (bh - CAP_HEIGHT * size) / 2.0;
    let comb = w.max_len.filter(|n| *n > 0 && w.flags & FF_COMB != 0 && !w.choice);
    let runs = match comb {
        // One character per cell, centred in it.
        Some(cells) => {
            let cell = bw / cells as f32;
            text.chars()
                .take(cells)
                .enumerate()
                .map(|(i, c)| {
                    let s = c.to_string();
                    (cell * i as f32 + (cell - font.width(&s) * size) / 2.0, y, s)
                })
                .collect()
        }
        None if text.is_empty() => Vec::new(),
        None => vec![(align(size, &text), y, text)],
    };
    Layout { size, runs, marks }
}

fn fmt(v: f32) -> String {
    let s = format!("{:.3}", v + 0.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

fn color_op(c: &[f32], stroke: bool) -> Option<String> {
    let op = match (c.len(), stroke) {
        (1, false) => "g",
        (1, true) => "G",
        (3, false) => "rg",
        (3, true) => "RG",
        (4, false) => "k",
        (4, true) => "K",
        _ => return None,
    };
    Some(format!("{} {op}", c.iter().map(|v| fmt(*v)).collect::<Vec<_>>().join(" ")))
}

fn content(w: &Widget, font: &FieldFont, font_key: &str, lay: &Layout) -> Vec<u8> {
    let (bw, bh) = w.text_box();
    let mut c: Vec<u8> = Vec::new();
    let mut put = |s: String| {
        c.extend_from_slice(s.as_bytes());
        c.push(b'\n');
    };
    if let Some(op) = color_op(&w.background, false) {
        put(format!("q {op} 0 0 {} {} re f Q", fmt(bw), fmt(bh)));
    }
    if w.border > 0.0 {
        if let Some(op) = color_op(&w.border_color, true) {
            let b = w.border;
            let dash = w.dash.as_ref().map_or(String::new(), |d| {
                format!(" [{}] 0 d", d.iter().map(|v| fmt(*v)).collect::<Vec<_>>().join(" "))
            });
            if w.underline {
                put(format!("q {op} {} w{dash} 0 {} m {} {} l S Q", fmt(b), fmt(b / 2.0), fmt(bw), fmt(b / 2.0)));
            } else {
                put(format!(
                    "q {op} {} w{dash} {} {} {} {} re S Q",
                    fmt(b), fmt(b / 2.0), fmt(b / 2.0), fmt(bw - b), fmt(bh - b)
                ));
            }
        }
    }
    let inset = w.border.max(1.0);
    put("/Tx BMC".into());
    put(format!("q {} {} {} {} re W n", fmt(inset), fmt(inset), fmt((bw - 2.0 * inset).max(0.0)), fmt((bh - 2.0 * inset).max(0.0))));
    for (y, h) in &lay.marks {
        put(format!("0.6 0.757 0.855 rg {} {} {} {} re f", fmt(inset), fmt(*y), fmt(bw - 2.0 * inset), fmt(*h)));
    }
    if !lay.runs.is_empty() {
        put(format!("BT /{font_key} {} Tf {}", fmt(lay.size), w.da.color_ops));
        for (x, y, text) in &lay.runs {
            let mut line = format!("1 0 0 1 {} {} Tm ", fmt(*x), fmt(*y)).into_bytes();
            line.extend_from_slice(&font.encode(text));
            line.extend_from_slice(b" Tj\n");
            c.extend_from_slice(&line);
        }
        c.extend_from_slice(b"ET\n");
    }
    c.extend_from_slice(b"Q\nEMC\n");
    c
}

/// Maps the text box onto the widget rectangle for /MK /R.
fn form_matrix(turns: u8, rect_w: f32, rect_h: f32) -> [f32; 6] {
    match turns % 4 {
        1 => [0.0, 1.0, -1.0, 0.0, rect_w, 0.0],
        2 => [-1.0, 0.0, 0.0, -1.0, rect_w, rect_h],
        3 => [0.0, -1.0, 1.0, 0.0, 0.0, rect_h],
        _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    }
}

/// (Re)writes the normal appearance of the text or choice widget `id` from
/// the field's current value.
pub(crate) fn write_appearance(doc: &mut Document, id: ObjectId) -> MutateResult<()> {
    let w = doc
        .get_dictionary(id)
        .ok()
        .and_then(|d| read_widget(doc, d))
        .ok_or_else(|| MutateError::Parse("widget has no usable text appearance".into()))?;
    let shown = if w.list() { w.options.join("") } else { w.value.clone() };
    let font = field_font(doc, &w.da.font, &shown);
    let lay = layout(&w, &font);
    let (key, font_id) = materialize(doc, &w.da.font, &font)?;
    let (bw, bh) = w.text_box();
    let mut fonts = Dictionary::new();
    fonts.set(key.as_bytes().to_vec(), Object::Reference(font_id));
    let mut xobj = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "BBox" => vec![Object::Real(0.0), Object::Real(0.0), Object::Real(bw), Object::Real(bh)],
        "Resources" => dictionary! { "Font" => fonts },
    };
    if w.mk_turns != 0 {
        let m = form_matrix(w.mk_turns, w.rect[2] - w.rect[0], w.rect[3] - w.rect[1]);
        xobj.set("Matrix", m.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>());
    }
    let stream = doc.add_object(Stream::new(xobj, content(&w, &font, &key, &lay)));
    if let Ok(d) = doc.get_dictionary_mut(id) {
        d.set("AP", dictionary! { "N" => stream });
    }
    Ok(())
}

/// Font size of the first `Tf` in a content stream.
fn stream_font_size(content: &[u8]) -> Option<f32> {
    let text = String::from_utf8_lossy(content);
    let tokens: Vec<&str> = text.split_ascii_whitespace().collect();
    let i = tokens.iter().position(|t| *t == "Tf")?;
    tokens.get(i.checked_sub(1)?)?.parse::<f32>().ok().filter(|s| *s > 0.0)
}

/// Font size the widget's present appearance draws its text with, in the
/// units of the widget rectangle.
fn appearance_font_size(doc: &Document, widget: &Dictionary, w: &Widget) -> Option<f32> {
    let n = get(doc, dict(doc, widget, b"AP")?, b"N")?;
    let Object::Stream(s) = n else { return None };
    let data = s.decompressed_content().unwrap_or_else(|_| s.content.clone());
    let size = stream_font_size(&data)?;
    // The form is fitted into the rectangle: /Matrix and a /BBox of another
    // size scale the text.
    let m = s.dict.get(b"Matrix").ok().map(|o| numbers(doc, o)).filter(|m| m.len() == 6);
    let bbox = s.dict.get(b"BBox").ok().map(|o| numbers(doc, o)).filter(|b| b.len() == 4);
    let scale = match bbox {
        Some(b) => {
            let m = m.unwrap_or(vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
            let pts: Vec<(f32, f32)> = [(b[0], b[1]), (b[0], b[3]), (b[2], b[1]), (b[2], b[3])]
                .iter()
                .map(|(x, y)| (m[0] * x + m[2] * y, m[1] * x + m[3] * y))
                .collect();
            let span = |f: fn(&(f32, f32)) -> f32| {
                let v: Vec<f32> = pts.iter().map(f).collect();
                v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
            };
            let (tw, th) = (span(|p| p.0), span(|p| p.1));
            let unit = (m[0] * m[3] - m[1] * m[2]).abs().sqrt();
            if tw > 0.0 && th > 0.0 {
                // Uniform part of the fit (fields are not drawn distorted).
                unit * ((w.rect[2] - w.rect[0]) / tw).min((w.rect[3] - w.rect[1]) / th)
            } else {
                unit
            }
        }
        None => 1.0,
    };
    Some(size * scale).filter(|s| s.is_finite() && *s > 0.0)
}

/// How every text and choice widget sets its text, keyed by page index and
/// position in the page's /Annots.
pub fn text_styles(doc: &Document) -> HashMap<(u32, u32), FieldTextStyle> {
    let mut out = HashMap::new();
    for (page_no, page_id) in doc.get_pages() {
        let Ok(page) = doc.get_dictionary(page_id) else { continue };
        let Some(Object::Array(annots)) = get(doc, page, b"Annots") else { continue };
        let page_turns = crate::page_rotation(doc, page_id) / 90;
        for (i, a) in annots.iter().enumerate() {
            let Object::Dictionary(d) = deref(doc, a) else { continue };
            if get(doc, d, b"Subtype").and_then(|o| o.as_name().ok()) != Some(b"Widget".as_slice()) {
                continue;
            }
            let Some(w) = read_widget(doc, d) else { continue };
            let shown = if w.list() { w.options.join("") } else { w.value.clone() };
            let font = field_font(doc, &w.da.font, &shown);
            let size = appearance_font_size(doc, d, &w).unwrap_or_else(|| layout(&w, &font).size);
            out.insert(
                (page_no - 1, i as u32),
                FieldTextStyle {
                    font_size_pt: size,
                    auto_size: w.da.size <= 0.0,
                    font_family: font.family.to_string(),
                    bold: font.bold,
                    italic: font.italic,
                    color: w.da.rgb,
                    align: w.q,
                    padding_pt: text_padding(w.border),
                    turns: (page_turns - w.mk_turns as i32).rem_euclid(4) as u8,
                },
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn da_gives_font_size_and_colour() {
        let da = parse_da("/Helv 12 Tf 0 g");
        assert_eq!((da.font.as_str(), da.size, da.color_ops.as_str()), ("Helv", 12.0, "0 g"));
        let da = parse_da("0.2 0.4 1 rg /F1 0 Tf");
        assert_eq!((da.font.as_str(), da.size), ("F1", 0.0));
        assert_eq!(da.rgb, [0.2, 0.4, 1.0]);
        assert_eq!(da.color_ops, "0.2 0.4 1 rg");
        assert_eq!(parse_da("0 0 0 1 k /TiRo 9.5 Tf").rgb, [0.0, 0.0, 0.0]);
        assert_eq!(parse_da(""), Da::default());
    }

    #[test]
    fn font_names_map_to_standard_families() {
        assert_eq!(classify_font("ArialMT"), ("Helv", false, false));
        assert_eq!(classify_font("ABCDEF+TimesNewRomanPS-BoldItalicMT"), ("Times", true, true));
        assert_eq!(classify_font("CourierNewPS-BoldMT"), ("Cour", true, false));
        assert_eq!(classify_font("NotoSans-Italic"), ("Helv", false, true));
        assert_eq!(classify_font("TiBo"), ("Times", true, false));
    }

    #[test]
    fn auto_size_fills_the_height_and_shrinks_to_the_width() {
        // 22 pt high, 1 pt border: (22 - 2 - 2) / 1.15.
        assert_eq!(auto_size_single(22.0, 1.0, 500.0, 5.0), 15.65);
        // A long text is limited by the width: 100 pt for a width of 10 em.
        assert_eq!(auto_size_single(22.0, 1.0, 100.0, 10.0), 10.0);
        // Never below the minimum.
        assert_eq!(auto_size_single(22.0, 1.0, 10.0, 50.0), MIN_AUTO_SIZE);
        assert_eq!(auto_size_single(3.0, 0.0, 100.0, 1.0), MIN_AUTO_SIZE);
        // Twice the box, twice the size: the size follows the field.
        let a = auto_size_single(20.0, 0.0, 1000.0, 3.0);
        let b = auto_size_single(40.0, 0.0, 1000.0, 3.0);
        assert!((b / a - 38.0 / 18.0).abs() < 0.01, "{a} {b}");
    }

    #[test]
    fn multiline_auto_size_steps_down_until_the_lines_fit() {
        assert_eq!(auto_size_lines(100.0, |_| 2), MULTILINE_AUTO_SIZE);
        // 5 lines in 46 pt: 8 pt (5 * 8 * 1.15 = 46).
        assert_eq!(auto_size_lines(46.0, |_| 5), 8.0);
        assert_eq!(auto_size_lines(1.0, |_| 50), MIN_AUTO_SIZE);
    }

    #[test]
    fn wrapping_keeps_breaks_and_splits_long_words() {
        let m = |s: &str| s.chars().count() as f32;
        assert_eq!(wrap_text("aa bb cc", 5.0, m), ["aa bb", "cc"]);
        assert_eq!(wrap_text("one\r\ntwo\rthree", 10.0, m), ["one", "two", "three"]);
        assert_eq!(wrap_text("abcdefgh", 3.0, m), ["abc", "def", "gh"]);
        assert_eq!(wrap_text("", 3.0, m), [""]);
        assert_eq!(wrap_text("a\n\nb", 3.0, m), ["a", "", "b"]);
    }

    #[test]
    fn size_is_read_from_a_content_stream() {
        assert_eq!(stream_font_size(b"/Tx BMC q BT /Helv 11.5 Tf 0 g 2 3 Td (x) Tj ET Q EMC"), Some(11.5));
        assert_eq!(stream_font_size(b"q 0 0 10 10 re f Q"), None);
    }

    #[test]
    fn rotated_text_box_maps_onto_the_rectangle() {
        // /MK /R 90: a 20 x 100 rectangle holds a 100 x 20 text box.
        let m = form_matrix(1, 20.0, 100.0);
        let p = |x: f32, y: f32| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
        assert_eq!(p(0.0, 0.0), (20.0, 0.0));
        assert_eq!(p(100.0, 20.0), (0.0, 100.0));
        let m = form_matrix(3, 20.0, 100.0);
        let p = |x: f32, y: f32| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
        assert_eq!(p(0.0, 0.0), (0.0, 100.0));
        assert_eq!(p(100.0, 20.0), (20.0, 0.0));
    }
}
