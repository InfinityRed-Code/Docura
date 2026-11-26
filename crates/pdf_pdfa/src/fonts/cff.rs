//! The little of the Compact Font Format that is needed to drop glyph
//! outlines without moving anything else: where the charstrings are.

use std::collections::BTreeSet;

fn be(d: &[u8], pos: usize, size: usize) -> Option<usize> {
    let b = d.get(pos..pos + size)?;
    let mut v = 0usize;
    for x in b {
        v = v << 8 | *x as usize;
    }
    Some(v)
}

/// Byte ranges of the items of the INDEX at `pos`, and where it ends.
fn index(d: &[u8], pos: usize) -> Option<(Vec<(usize, usize)>, usize)> {
    let count = be(d, pos, 2)?;
    if count == 0 {
        return Some((Vec::new(), pos + 2));
    }
    let size = *d.get(pos + 2)? as usize;
    if !(1..=4).contains(&size) {
        return None;
    }
    // Offsets count from the byte before the data.
    let base = pos + 3 + (count + 1) * size - 1;
    let mut items = Vec::with_capacity(count);
    let mut prev = be(d, pos + 3, size)?;
    for i in 1..=count {
        let next = be(d, pos + 3 + i * size, size)?;
        if next < prev || base + next > d.len() {
            return None;
        }
        items.push((base + prev, base + next));
        prev = next;
    }
    Some((items, base + prev))
}

/// The integer operand in front of the one-byte operator `want` in a DICT.
fn dict_operand(d: &[u8], want: u8) -> Option<i64> {
    let mut i = 0;
    let mut last = None;
    while i < d.len() {
        let b = d[i];
        match b {
            12 => {
                // Two-byte operator.
                i += 2;
                last = None;
            }
            0..=21 => {
                if b == want {
                    return last;
                }
                i += 1;
                last = None;
            }
            28 => {
                last = Some(i16::from_be_bytes([*d.get(i + 1)?, *d.get(i + 2)?]) as i64);
                i += 3;
            }
            29 => {
                last = Some(i32::from_be_bytes([*d.get(i + 1)?, *d.get(i + 2)?, *d.get(i + 3)?, *d.get(i + 4)?]) as i64);
                i += 5;
            }
            30 => {
                // Real number: nibbles up to the end marker 0xF.
                i += 1;
                loop {
                    let n = *d.get(i)?;
                    i += 1;
                    if n & 0x0F == 0x0F || n >> 4 == 0x0F {
                        break;
                    }
                }
                last = None;
            }
            32..=246 => {
                last = Some(b as i64 - 139);
                i += 1;
            }
            247..=250 => {
                last = Some((b as i64 - 247) * 256 + *d.get(i + 1)? as i64 + 108);
                i += 2;
            }
            251..=254 => {
                last = Some(-(b as i64 - 251) * 256 - *d.get(i + 1)? as i64 - 108);
                i += 2;
            }
            _ => return None,
        }
    }
    None
}

/// Byte ranges of the glyph programs (CharStrings INDEX) of a CFF font.
pub fn charstrings(cff: &[u8]) -> Option<Vec<(usize, usize)>> {
    if *cff.first()? != 1 {
        return None;
    }
    let header = *cff.get(2)? as usize;
    let (_, after_names) = index(cff, header)?;
    let (top, _) = index(cff, after_names)?;
    let (a, b) = *top.first()?;
    let offset = dict_operand(&cff[a..b], 17)?;
    if offset <= 0 {
        return None;
    }
    let (items, _) = index(cff, offset as usize)?;
    Some(items)
}

/// Replace the outline of every glyph outside `keep` by an empty one
/// (`endchar`). Nothing moves: offsets, glyph count, names and CIDs stay
/// as they are, and the freed bytes compress to almost nothing. Glyph 0
/// (.notdef) always stays. Returns how many outlines were removed.
pub fn blank(cff: &mut [u8], keep: &BTreeSet<u16>) -> Option<usize> {
    const ENDCHAR: u8 = 14;
    let ranges = charstrings(cff)?;
    let mut removed = 0;
    for (gid, (a, b)) in ranges.into_iter().enumerate() {
        if gid == 0 || keep.contains(&(gid as u16)) {
            continue;
        }
        let glyph = &mut cff[a..b];
        let mut empty = true;
        for x in glyph.iter() {
            empty &= *x == ENDCHAR;
        }
        if !empty {
            glyph.fill(ENDCHAR);
            removed += 1;
        }
    }
    Some(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ttf_parser::{GlyphId, OutlineBuilder};

    #[test]
    fn dict_numbers() {
        // 100, -100, 1000, -1000 (two-byte forms), 30000 (28), 100000 (29), real 1.5, then operator 17.
        assert_eq!(dict_operand(&[239, 17], 17), Some(100));
        assert_eq!(dict_operand(&[39, 17], 17), Some(-100));
        assert_eq!(dict_operand(&[250, 124, 17], 17), Some(1000));
        assert_eq!(dict_operand(&[254, 124, 17], 17), Some(-1000));
        assert_eq!(dict_operand(&[28, 0x75, 0x30, 17], 17), Some(30000));
        assert_eq!(dict_operand(&[29, 0, 1, 0x86, 0xA0, 17], 17), Some(100000));
        // Other operators and a real number in front; operand must be an integer.
        assert_eq!(dict_operand(&[139, 5, 12, 30, 30, 0x1A, 0x5F, 15, 239, 17], 17), Some(100));
        assert_eq!(dict_operand(&[30, 0x1A, 0x5F, 17], 17), None);
        assert_eq!(dict_operand(&[139, 15], 17), None);
        assert_eq!(dict_operand(&[255], 17), None);
        assert_eq!(dict_operand(&[28, 1], 17), None);
        assert_eq!(dict_operand(&[30, 0x12], 17), None);
    }

    #[test]
    fn index_layout() {
        // Two items "ab", "c" with one-byte offsets.
        let d = [0, 2, 1, 1, 3, 4, b'a', b'b', b'c', 9];
        assert_eq!(index(&d, 0), Some((vec![(6, 8), (8, 9)], 9)));
        assert_eq!(index(&[0, 0, 7], 0), Some((Vec::new(), 2)));
        // Offset size 0, descending offsets, data beyond the end, truncated header.
        assert_eq!(index(&[0, 1, 0, 1, 2], 0), None);
        assert_eq!(index(&[0, 1, 1, 3, 1, 0, 0], 0), None);
        assert_eq!(index(&[0, 1, 1, 1, 9], 0), None);
        assert_eq!(index(&[0], 0), None);
        assert_eq!(index(&[0, 1, 1, 1], 0), None);
    }

    #[test]
    fn blanks_unused_glyphs_in_place() {
        let font = crate::common::minimal_cff("T", &[('A', 600), ('B', 700), ('C', 650)]);
        let ranges = charstrings(&font).unwrap();
        assert_eq!(ranges.len(), 4);
        let mut slim = font.clone();
        assert_eq!(blank(&mut slim, &BTreeSet::from([2u16])), Some(2));
        assert_eq!(slim.len(), font.len());
        // A second pass finds nothing left to remove.
        assert_eq!(blank(&mut slim, &BTreeSet::from([2u16])), Some(0));
        let (old, new) = (ttf_parser::cff::Table::parse(&font).unwrap(), ttf_parser::cff::Table::parse(&slim).unwrap());
        assert_eq!(new.number_of_glyphs(), 4);
        struct Count(usize);
        impl ttf_parser::OutlineBuilder for Count {
            fn move_to(&mut self, _: f32, _: f32) {
                self.0 += 1;
            }
            fn line_to(&mut self, _: f32, _: f32) {
                self.0 += 1;
            }
            fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
                self.0 += 1;
            }
            fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
                self.0 += 1;
            }
            fn close(&mut self) {
                self.0 += 1;
            }
        }
        let segments = |t: &ttf_parser::cff::Table, g: u16| {
            let mut c = Count(0);
            let _ = t.outline(GlyphId(g), &mut c);
            // (CFF outlines have no quadratic segments.)
            c.quad_to(0.0, 0.0, 0.0, 0.0);
            c.0 - 1
        };
        // Kept glyph unchanged, the others empty, names untouched.
        assert!(segments(&old, 2) > 0);
        assert_eq!(segments(&new, 2), segments(&old, 2));
        assert_eq!(segments(&new, 1), 0);
        assert_eq!(segments(&new, 3), 0);
        assert_eq!(new.glyph_name(GlyphId(1)), Some("A"));
        assert_eq!(new.glyph_width(GlyphId(2)), old.glyph_width(GlyphId(2)));

        // CID-keyed fonts work the same way.
        let cid = crate::common::edge::minimal_cid_cff("C", &[600, 700]);
        let mut slim = cid.clone();
        assert_eq!(blank(&mut slim, &BTreeSet::new()), Some(2));
        assert_eq!(ttf_parser::cff::Table::parse(&slim).unwrap().glyph_cid(GlyphId(2)), Some(2));

        // Not a CFF font.
        assert_eq!(charstrings(b""), None);
        assert_eq!(charstrings(&[2, 0, 4, 1]), None);
        assert_eq!(charstrings(&[1, 0]), None);
        assert_eq!(blank(&mut [1u8, 0, 4, 1, 0, 0, 0, 0], &BTreeSet::new()), None);
        // Top DICT without a CharStrings entry, or with offset 0.
        let mut no_cs = vec![1u8, 0, 4, 1, 0, 0, 0, 1, 1, 1, 2, 139];
        assert_eq!(charstrings(&no_cs), None);
        no_cs.splice(8.., [1, 1, 3, 139, 17]);
        assert_eq!(charstrings(&no_cs), None);
    }
}
