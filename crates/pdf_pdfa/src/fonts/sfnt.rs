//! TrueType (glyf) font subsetting.
//!
//! Builds a new font from a list of source glyphs. Each output glyph can
//! get its own advance width, so the font program always agrees with the
//! widths in the PDF font dictionary (PDF/A width consistency) without
//! moving any text. Composite glyph components are pulled in and
//! renumbered. Output tables: cmap (optional), glyf, head, hhea, hmtx,
//! loca, maxp, name, OS/2, post (format 3), cvt/fpgm/prep when present.

use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SfntError {
    #[error("not a TrueType font: {0}")]
    Invalid(&'static str),
    #[error("font has no glyf outlines (CFF)")]
    NoGlyf,
}

pub type SResult<T> = std::result::Result<T, SfntError>;

/// A parsed sfnt: table slices of one face.
pub struct Sfnt<'a> {
    pub data: &'a [u8],
    tables: BTreeMap<[u8; 4], (usize, usize)>,
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*d.get(o)?, *d.get(o + 1)?]))
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*d.get(o)?, *d.get(o + 1)?, *d.get(o + 2)?, *d.get(o + 3)?]))
}

impl<'a> Sfnt<'a> {
    /// Parse face `index` (TrueType collections supported).
    pub fn parse(data: &'a [u8], index: u32) -> SResult<Self> {
        let mut off = 0usize;
        if data.get(0..4) == Some(b"ttcf") {
            let n = u32_at(data, 8).ok_or(SfntError::Invalid("ttc header"))?;
            if index >= n {
                return Err(SfntError::Invalid("ttc index"));
            }
            off = u32_at(data, 12 + 4 * index as usize).ok_or(SfntError::Invalid("ttc offset"))? as usize;
        }
        let ver = u32_at(data, off).ok_or(SfntError::Invalid("header"))?;
        if ver != 0x0001_0000 && ver != 0x7472_7565 && ver != 0x4F54_544F {
            return Err(SfntError::Invalid("version"));
        }
        let num = u16_at(data, off + 4).ok_or(SfntError::Invalid("numTables"))? as usize;
        let mut tables = BTreeMap::new();
        for i in 0..num {
            let r = off + 12 + 16 * i;
            let tag: [u8; 4] = data.get(r..r + 4).ok_or(SfntError::Invalid("table record"))?.try_into().unwrap();
            let o = u32_at(data, r + 8).ok_or(SfntError::Invalid("table record"))? as usize;
            let l = u32_at(data, r + 12).ok_or(SfntError::Invalid("table record"))? as usize;
            if o.checked_add(l).is_none_or(|e| e > data.len()) {
                // Some fonts overstate the last table's length by padding.
                let l2 = data.len().saturating_sub(o);
                if o > data.len() {
                    continue;
                }
                tables.insert(tag, (o, l2));
                continue;
            }
            tables.insert(tag, (o, l));
        }
        Ok(Sfnt { data, tables })
    }

    pub fn table(&self, tag: &[u8; 4]) -> Option<&'a [u8]> {
        let (o, l) = *self.tables.get(tag)?;
        self.data.get(o..o + l)
    }

    pub fn has(&self, tag: &[u8; 4]) -> bool {
        self.tables.contains_key(tag)
    }

    pub fn num_glyphs(&self) -> u16 {
        self.table(b"maxp").and_then(|m| u16_at(m, 4)).unwrap_or(0)
    }

    pub fn units_per_em(&self) -> u16 {
        self.table(b"head").and_then(|h| u16_at(h, 18)).filter(|u| *u >= 16).unwrap_or(1000)
    }

    fn long_loca(&self) -> bool {
        self.table(b"head").and_then(|h| u16_at(h, 50)).unwrap_or(0) == 1
    }

    /// Byte range of glyph `gid` inside glyf.
    pub fn glyph_range(&self, gid: u16) -> Option<(usize, usize)> {
        let loca = self.table(b"loca")?;
        let g = gid as usize;
        let (a, b) = if self.long_loca() {
            (u32_at(loca, g * 4)? as usize, u32_at(loca, g * 4 + 4)? as usize)
        } else {
            (u16_at(loca, g * 2)? as usize * 2, u16_at(loca, g * 2 + 2)? as usize * 2)
        };
        if b < a {
            return None;
        }
        Some((a, b))
    }

    pub fn glyph_data(&self, gid: u16) -> &'a [u8] {
        let Some(glyf) = self.table(b"glyf") else { return &[] };
        match self.glyph_range(gid) {
            Some((a, b)) => glyf.get(a..b).unwrap_or(&[]),
            None => &[],
        }
    }

    /// Advance and left side bearing of `gid` in font units.
    pub fn h_metrics(&self, gid: u16) -> (u16, i16) {
        let hhea = self.table(b"hhea");
        let hmtx = self.table(b"hmtx");
        let n = hhea.and_then(|h| u16_at(h, 34)).unwrap_or(0) as usize;
        let (Some(hmtx), true) = (hmtx, n > 0) else { return (0, 0) };
        let g = gid as usize;
        if g < n {
            (u16_at(hmtx, g * 4).unwrap_or(0), u16_at(hmtx, g * 4 + 2).unwrap_or(0) as i16)
        } else {
            let adv = u16_at(hmtx, (n - 1) * 4).unwrap_or(0);
            let lsb = u16_at(hmtx, n * 4 + (g - n) * 2).unwrap_or(0) as i16;
            (adv, lsb)
        }
    }

    /// Component glyph ids of a composite glyph.
    pub fn components(&self, gid: u16) -> Vec<u16> {
        let g = self.glyph_data(gid);
        let mut out = Vec::new();
        if g.len() < 10 || (u16_at(g, 0).unwrap_or(0) as i16) >= 0 {
            return out;
        }
        let mut p = 10;
        loop {
            let (Some(flags), Some(cg)) = (u16_at(g, p), u16_at(g, p + 2)) else { break };
            out.push(cg);
            p += 4;
            p += if flags & 0x0001 != 0 { 4 } else { 2 };
            if flags & 0x0008 != 0 {
                p += 2;
            } else if flags & 0x0040 != 0 {
                p += 4;
            } else if flags & 0x0080 != 0 {
                p += 8;
            }
            if flags & 0x0020 == 0 {
                break;
            }
        }
        out
    }
}

/// One output glyph: the source glyph (None = empty glyph) and an optional
/// advance in font units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutGlyph {
    pub src: Option<u16>,
    pub advance: Option<u16>,
}

/// The cmap to write: one (platform, encoding) subtable, code → new gid.
pub struct CmapSpec {
    pub platform: u16,
    pub encoding: u16,
    pub map: BTreeMap<u32, u16>,
}

pub struct Subset {
    pub data: Vec<u8>,
    /// Number of glyphs in the new font (explicit + pulled-in components).
    pub num_glyphs: u16,
}

fn rewrite_components(g: &[u8], map: &dyn Fn(u16) -> u16) -> Vec<u8> {
    let mut out = g.to_vec();
    if g.len() < 10 || (u16_at(g, 0).unwrap_or(0) as i16) >= 0 {
        return out;
    }
    let mut p = 10;
    loop {
        let (Some(flags), Some(cg)) = (u16_at(g, p), u16_at(g, p + 2)) else { break };
        let n = map(cg);
        out[p + 2..p + 4].copy_from_slice(&n.to_be_bytes());
        p += 4;
        p += if flags & 0x0001 != 0 { 4 } else { 2 };
        if flags & 0x0008 != 0 {
            p += 2;
        } else if flags & 0x0040 != 0 {
            p += 4;
        } else if flags & 0x0080 != 0 {
            p += 8;
        }
        if flags & 0x0020 == 0 {
            break;
        }
    }
    out
}

fn checksum(d: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for c in d.chunks(4) {
        let mut b = [0u8; 4];
        b[..c.len()].copy_from_slice(c);
        sum = sum.wrapping_add(u32::from_be_bytes(b));
    }
    sum
}

/// cmap with one format 4 (BMP) or format 12 subtable.
fn build_cmap(spec: &CmapSpec) -> Vec<u8> {
    let wide = spec.map.keys().any(|&c| c > 0xFFFF);
    let mut sub = Vec::new();
    if wide {
        // Format 12: one group per code (simple and always valid).
        let groups: Vec<(u32, u16)> = spec.map.iter().map(|(&c, &g)| (c, g)).collect();
        sub.extend_from_slice(&12u16.to_be_bytes());
        sub.extend_from_slice(&0u16.to_be_bytes());
        sub.extend_from_slice(&((16 + 12 * groups.len()) as u32).to_be_bytes());
        sub.extend_from_slice(&0u32.to_be_bytes());
        sub.extend_from_slice(&(groups.len() as u32).to_be_bytes());
        for (c, g) in groups {
            sub.extend_from_slice(&c.to_be_bytes());
            sub.extend_from_slice(&c.to_be_bytes());
            sub.extend_from_slice(&(g as u32).to_be_bytes());
        }
    } else {
        // Format 4 with one segment per contiguous run of codes.
        let mut segs: Vec<(u16, u16, Vec<u16>)> = Vec::new();
        for (&c, &g) in &spec.map {
            let c = c as u16;
            if let Some(last) = segs.last_mut() {
                if last.1.wrapping_add(1) == c && c != 0 {
                    last.1 = c;
                    last.2.push(g);
                    continue;
                }
            }
            segs.push((c, c, vec![g]));
        }
        segs.push((0xFFFF, 0xFFFF, vec![0]));
        let n = segs.len();
        let mut ends = Vec::new();
        let mut starts = Vec::new();
        let mut deltas = Vec::new();
        let mut offsets = Vec::new();
        let mut glyph_array: Vec<u16> = Vec::new();
        for (i, (s, e, gs)) in segs.iter().enumerate() {
            ends.push(*e);
            starts.push(*s);
            let delta_ok = gs.windows(2).all(|w| w[1] == w[0].wrapping_add(1));
            if *s == 0xFFFF {
                deltas.push(1u16);
                offsets.push(0u16);
            } else if delta_ok {
                deltas.push(gs[0].wrapping_sub(*s));
                offsets.push(0);
            } else {
                deltas.push(0);
                // Offset from this idRangeOffset entry to glyphArray item.
                let off = (2 * (n - i) + 2 * glyph_array.len()) as u16;
                offsets.push(off);
                glyph_array.extend_from_slice(gs);
            }
        }
        let seg_x2 = (n * 2) as u16;
        let mut sr = 1u16;
        let mut es = 0u16;
        while sr * 2 <= n as u16 {
            sr *= 2;
            es += 1;
        }
        let search_range = sr * 2;
        let range_shift = seg_x2 - search_range;
        let len = 16 + 8 * n + 2 * glyph_array.len();
        sub.extend_from_slice(&4u16.to_be_bytes());
        sub.extend_from_slice(&(len as u16).to_be_bytes());
        sub.extend_from_slice(&0u16.to_be_bytes());
        sub.extend_from_slice(&seg_x2.to_be_bytes());
        sub.extend_from_slice(&search_range.to_be_bytes());
        sub.extend_from_slice(&es.to_be_bytes());
        sub.extend_from_slice(&range_shift.to_be_bytes());
        ends.iter().for_each(|v| sub.extend_from_slice(&v.to_be_bytes()));
        sub.extend_from_slice(&0u16.to_be_bytes());
        starts.iter().for_each(|v| sub.extend_from_slice(&v.to_be_bytes()));
        deltas.iter().for_each(|v| sub.extend_from_slice(&v.to_be_bytes()));
        offsets.iter().for_each(|v| sub.extend_from_slice(&v.to_be_bytes()));
        glyph_array.iter().for_each(|v| sub.extend_from_slice(&v.to_be_bytes()));
    }
    let mut t = Vec::new();
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&1u16.to_be_bytes());
    t.extend_from_slice(&spec.platform.to_be_bytes());
    t.extend_from_slice(&spec.encoding.to_be_bytes());
    t.extend_from_slice(&12u32.to_be_bytes());
    t.extend_from_slice(&sub);
    t
}

/// The naming table reduced to the identifying English names (family,
/// style, unique id, full name, version, PostScript name). System fonts
/// carry these in dozens of languages plus licence texts, often more bytes
/// than the glyphs of a small subset. A table that cannot be read is
/// copied unchanged.
pub fn slim_name(t: &[u8]) -> Vec<u8> {
    let be16 = |o: usize| -> Option<usize> { Some(u16::from_be_bytes([*t.get(o)?, *t.get(o + 1)?]) as usize) };
    let (Some(count), Some(storage)) = (be16(2), be16(4)) else { return t.to_vec() };
    let mut records = Vec::new();
    let mut strings = Vec::new();
    for i in 0..count {
        let r = 6 + 12 * i;
        let (Some(pid), Some(lang), Some(id), Some(len), Some(off)) = (be16(r), be16(r + 4), be16(r + 6), be16(r + 8), be16(r + 10)) else {
            return t.to_vec();
        };
        let english = (pid == 3 && lang == 0x409) || (pid == 1 && lang == 0);
        if !english || !(1..=6).contains(&id) {
            continue;
        }
        let Some(s) = t.get(storage + off..storage + off + len) else { return t.to_vec() };
        records.push((t[r..r + 6].to_vec(), id as u16, strings.len() as u16, len as u16));
        strings.extend_from_slice(s);
    }
    let mut out = Vec::with_capacity(6 + 12 * records.len() + strings.len());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&(records.len() as u16).to_be_bytes());
    out.extend_from_slice(&((6 + 12 * records.len()) as u16).to_be_bytes());
    for (head, id, off, len) in records {
        out.extend_from_slice(&head);
        out.extend_from_slice(&id.to_be_bytes());
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&off.to_be_bytes());
    }
    out.extend_from_slice(&strings);
    out
}

/// Build the subset. `glyphs[0]` should be the .notdef glyph.
pub fn subset(src: &Sfnt, glyphs: &[OutGlyph], cmap: Option<&CmapSpec>) -> SResult<Subset> {
    if !src.has(b"glyf") || !src.has(b"loca") {
        return Err(SfntError::NoGlyf);
    }
    let head = src.table(b"head").ok_or(SfntError::Invalid("head"))?;
    let hhea = src.table(b"hhea").ok_or(SfntError::Invalid("hhea"))?;
    let maxp = src.table(b"maxp").ok_or(SfntError::Invalid("maxp"))?;
    if head.len() < 54 || hhea.len() < 36 || maxp.len() < 6 {
        return Err(SfntError::Invalid("short tables"));
    }
    let n_src = src.num_glyphs();

    // Output order: explicit glyphs, then component closure.
    let mut out: Vec<OutGlyph> = glyphs.to_vec();
    let mut comp_map: BTreeMap<u16, u16> = BTreeMap::new();
    let mut i = 0;
    while i < out.len() {
        if let Some(sg) = out[i].src {
            for c in src.components(sg) {
                if c >= n_src {
                    continue;
                }
                if let std::collections::btree_map::Entry::Vacant(e) = comp_map.entry(c) {
                    let idx = out.len() as u16;
                    e.insert(idx);
                    out.push(OutGlyph { src: Some(c), advance: None });
                }
            }
        }
        i += 1;
        if out.len() > 65535 {
            return Err(SfntError::Invalid("too many glyphs"));
        }
    }
    let map_comp = |c: u16| -> u16 { *comp_map.get(&c).unwrap_or(&0) };

    let mut glyf = Vec::new();
    let mut loca: Vec<u32> = Vec::with_capacity(out.len() + 1);
    let mut hmtx = Vec::with_capacity(out.len() * 4);
    let mut max_adv = 0u16;
    for g in &out {
        loca.push(glyf.len() as u32);
        let (adv, lsb) = match g.src {
            Some(s) if s < n_src => {
                let data = src.glyph_data(s);
                glyf.extend_from_slice(&rewrite_components(data, &map_comp));
                while glyf.len() % 4 != 0 {
                    glyf.push(0);
                }
                src.h_metrics(s)
            }
            _ => (0, 0),
        };
        let adv = g.advance.unwrap_or(adv);
        max_adv = max_adv.max(adv);
        hmtx.extend_from_slice(&adv.to_be_bytes());
        hmtx.extend_from_slice(&lsb.to_be_bytes());
    }
    loca.push(glyf.len() as u32);
    let loca_bytes: Vec<u8> = loca.iter().flat_map(|v| v.to_be_bytes()).collect();

    let n = out.len() as u16;
    let mut head_t = head.to_vec();
    head_t[8..12].copy_from_slice(&0u32.to_be_bytes());
    head_t[50..52].copy_from_slice(&1u16.to_be_bytes());
    let mut hhea_t = hhea.to_vec();
    hhea_t[10..12].copy_from_slice(&max_adv.to_be_bytes());
    hhea_t[34..36].copy_from_slice(&n.to_be_bytes());
    let mut maxp_t = maxp.to_vec();
    maxp_t[4..6].copy_from_slice(&n.to_be_bytes());
    let mut post = vec![0u8; 32];
    post[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
    if let Some(p) = src.table(b"post") {
        if p.len() >= 32 {
            post[4..32].copy_from_slice(&p[4..32]);
        }
    }

    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"glyf", glyf),
        (*b"head", head_t),
        (*b"hhea", hhea_t),
        (*b"hmtx", hmtx),
        (*b"loca", loca_bytes),
        (*b"maxp", maxp_t),
        (*b"post", post),
    ];
    if let Some(c) = cmap {
        tables.push((*b"cmap", build_cmap(c)));
    }
    for tag in [b"cvt ", b"fpgm", b"prep", b"OS/2"] {
        if let Some(t) = src.table(tag) {
            tables.push((*tag, t.to_vec()));
        }
    }
    if let Some(t) = src.table(b"name") {
        tables.push((*b"name", slim_name(t)));
    }
    tables.sort_by(|a, b| a.0.cmp(&b.0));

    let num = tables.len() as u16;
    let mut sr = 1u16;
    let mut es = 0u16;
    while sr * 2 <= num {
        sr *= 2;
        es += 1;
    }
    let mut font = Vec::new();
    font.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    font.extend_from_slice(&num.to_be_bytes());
    font.extend_from_slice(&(sr * 16).to_be_bytes());
    font.extend_from_slice(&es.to_be_bytes());
    font.extend_from_slice(&(num * 16 - sr * 16).to_be_bytes());
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    let mut head_off = 0;
    for (tag, data) in &tables {
        font.extend_from_slice(tag);
        font.extend_from_slice(&checksum(data).to_be_bytes());
        font.extend_from_slice(&(offset as u32).to_be_bytes());
        font.extend_from_slice(&(data.len() as u32).to_be_bytes());
        if tag == b"head" {
            head_off = offset;
        }
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    font.extend_from_slice(&body);
    let adj = 0xB1B0_AFBAu32.wrapping_sub(checksum(&font));
    font[head_off + 8..head_off + 12].copy_from_slice(&adj.to_be_bytes());
    Ok(Subset { data: font, num_glyphs: n })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn roboto() -> &'static [u8] {
        pdf_fonts::family("Roboto").expect("bundled Roboto").face(false, false)
    }

    #[test]
    fn name_table_is_reduced_to_english_names() {
        let src = Sfnt::parse(roboto(), 0).unwrap();
        let full = src.table(b"name").unwrap();
        let slim = slim_name(full);
        assert!(slim.len() <= full.len());
        // Still a valid font with its PostScript name.
        let sub = subset(&src, &[OutGlyph { src: Some(0), advance: None }], None).unwrap();
        let face = ttf_parser::Face::parse(&sub.data, 0).unwrap();
        let ps = face.names().into_iter().find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME).and_then(|n| n.to_string());
        assert_eq!(ps.as_deref(), Some("Roboto-Regular"));
        assert!(face.names().into_iter().all(|n| (1..=6).contains(&n.name_id)));
        // Hand-made table: one English record kept, a German one and a licence text dropped.
        let mut t = vec![0, 0, 0, 3, 0, 42];
        for (lang, id, len, off) in [(0x409u16, 4u16, 2u16, 0u16), (0x407, 4, 2, 2), (0x409, 13, 2, 4)] {
            t.extend_from_slice(&[0, 3, 0, 1]);
            for v in [lang, id, len, off] {
                t.extend_from_slice(&v.to_be_bytes());
            }
        }
        t.extend_from_slice(b"ENDELI");
        let s = slim_name(&t);
        assert_eq!(&s[..6], &[0, 0, 0, 1, 0, 18]);
        assert_eq!(&s[18..], b"EN");
        // Unreadable tables are passed through.
        assert_eq!(slim_name(&[0, 0, 0]), vec![0, 0, 0]);
        assert_eq!(slim_name(&[0, 0, 0, 1, 0, 6]), vec![0, 0, 0, 1, 0, 6]);
        assert_eq!(slim_name(&t[..40]), t[..40].to_vec());
    }

    #[test]
    fn subset_roundtrip_with_cmap_and_advances() {
        let src = Sfnt::parse(roboto(), 0).unwrap();
        let face = ttf_parser::Face::parse(roboto(), 0).unwrap();
        let ga = face.glyph_index('A').unwrap().0;
        let gaa = face.glyph_index('Ä').unwrap().0; // usually composite
        let glyphs = vec![
            OutGlyph { src: Some(0), advance: None },
            OutGlyph { src: Some(ga), advance: Some(1234) },
            OutGlyph { src: Some(gaa), advance: None },
            OutGlyph { src: None, advance: Some(500) },
        ];
        let mut map = BTreeMap::new();
        map.insert(0xF041, 1);
        map.insert(0xF042, 3);
        map.insert(0xF0C4, 2);
        let cm = CmapSpec { platform: 3, encoding: 0, map };
        let s = subset(&src, &glyphs, Some(&cm)).unwrap();
        assert!(s.num_glyphs >= 4);
        let out = ttf_parser::Face::parse(&s.data, 0).unwrap();
        assert_eq!(out.number_of_glyphs(), s.num_glyphs);
        assert_eq!(out.glyph_hor_advance(ttf_parser::GlyphId(1)), Some(1234));
        assert_eq!(out.glyph_hor_advance(ttf_parser::GlyphId(3)), Some(500));
        let sym = out.tables().cmap.unwrap().subtables.into_iter().find(|s| s.platform_id == ttf_parser::PlatformId::Windows && s.encoding_id == 0).unwrap();
        assert_eq!(sym.glyph_index(0xF041), Some(ttf_parser::GlyphId(1)));
        assert_eq!(sym.glyph_index(0xF0C4), Some(ttf_parser::GlyphId(2)));
        assert_eq!(sym.glyph_index(0xF042), Some(ttf_parser::GlyphId(3)));
        assert!(out.glyph_bounding_box(ttf_parser::GlyphId(2)).is_some(), "composite outline survives");
        assert!(out.glyph_bounding_box(ttf_parser::GlyphId(3)).is_none());
        assert!(s.data.len() < roboto().len() / 4);
        let re = Sfnt::parse(&s.data, 0).unwrap();
        assert_eq!(re.units_per_em(), src.units_per_em());
    }

    #[test]
    fn wide_cmap_and_errors() {
        let src = Sfnt::parse(roboto(), 0).unwrap();
        let mut map = BTreeMap::new();
        map.insert(0x1F600, 1);
        map.insert(0x41, 1);
        let cm = CmapSpec { platform: 3, encoding: 10, map };
        let s = subset(&src, &[OutGlyph { src: Some(0), advance: None }, OutGlyph { src: Some(1), advance: None }], Some(&cm)).unwrap();
        let out = ttf_parser::Face::parse(&s.data, 0).unwrap();
        assert_eq!(out.glyph_index('A'), Some(ttf_parser::GlyphId(1)));
        assert!(Sfnt::parse(b"nope", 0).is_err());
        assert!(Sfnt::parse(b"ttcf\0\0\0\0\0\0\0\0", 0).is_err());
    }
}
