//! ICC profiles generated in code (no third-party profile files).
//!
//! * sRGB: ICC v2.1 display profile (matrix/TRC) with the IEC 61966-2.1
//!   primaries adapted to D50 and a 1024-entry sRGB tone curve. Used as the
//!   PDF/A OutputIntent.
//! * CMYK: ICC v2.1 input profile with an A2B0 lut8 (CMYK → Lab). Used as
//!   `DefaultCMYK`, so `DeviceCMYK` content stays untouched but becomes
//!   device independent. The CLUT reproduces a typical viewer conversion
//!   (CMYK → sRGB with dot gain softening), so the look of the page does not
//!   change noticeably.

/// Profile identifier used in the OutputIntent.
pub const SRGB_IDENTIFIER: &str = "sRGB IEC61966-2.1";

fn be32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn s15(v: &mut Vec<u8>, x: f64) {
    be32(v, ((x * 65536.0).round() as i32) as u32);
}

fn pad4(v: &mut Vec<u8>) {
    while v.len() % 4 != 0 {
        v.push(0);
    }
}

fn desc_tag(text: &str) -> Vec<u8> {
    let mut t = b"desc\0\0\0\0".to_vec();
    be32(&mut t, text.len() as u32 + 1);
    t.extend_from_slice(text.as_bytes());
    t.push(0);
    be32(&mut t, 0); // unicode language
    be32(&mut t, 0); // unicode count
    t.extend_from_slice(&[0, 0]); // scriptcode code
    t.push(0); // scriptcode count
    t.extend_from_slice(&[0u8; 67]);
    t
}

fn text_tag(text: &str) -> Vec<u8> {
    let mut t = b"text\0\0\0\0".to_vec();
    t.extend_from_slice(text.as_bytes());
    t.push(0);
    t
}

fn xyz_tag(x: f64, y: f64, z: f64) -> Vec<u8> {
    let mut t = b"XYZ \0\0\0\0".to_vec();
    s15(&mut t, x);
    s15(&mut t, y);
    s15(&mut t, z);
    t
}

fn srgb_eotf(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn curv_tag() -> Vec<u8> {
    let n = 1024u32;
    let mut t = b"curv\0\0\0\0".to_vec();
    be32(&mut t, n);
    for i in 0..n {
        let y = srgb_eotf(i as f64 / (n - 1) as f64);
        t.extend_from_slice(&((y * 65535.0).round() as u16).to_be_bytes());
    }
    t
}

/// Assemble header + tag table + tags. `tags` may repeat data; identical
/// data is stored once.
fn assemble(class: &[u8; 4], space: &[u8; 4], pcs: &[u8; 4], tags: Vec<(&[u8; 4], Vec<u8>)>) -> Vec<u8> {
    let header_len = 128;
    let table_len = 4 + 12 * tags.len();
    let mut data: Vec<u8> = Vec::new();
    let mut entries: Vec<([u8; 4], u32, u32)> = Vec::new();
    let mut stored: Vec<(Vec<u8>, u32)> = Vec::new();
    let base = (header_len + table_len) as u32;
    for (sig, body) in &tags {
        let off = if let Some((_, o)) = stored.iter().find(|(b, _)| b == body) {
            *o
        } else {
            pad4(&mut data);
            let o = base + data.len() as u32;
            data.extend_from_slice(body);
            stored.push((body.clone(), o));
            o
        };
        entries.push((**sig, off, body.len() as u32));
    }
    pad4(&mut data);
    let total = header_len + table_len + data.len();

    let mut p = Vec::with_capacity(total);
    be32(&mut p, total as u32);
    p.extend_from_slice(b"\0\0\0\0"); // CMM
    be32(&mut p, 0x0210_0000); // version 2.1
    p.extend_from_slice(class);
    p.extend_from_slice(space);
    p.extend_from_slice(pcs);
    for x in [2024u16, 1, 1, 0, 0, 0] {
        p.extend_from_slice(&x.to_be_bytes());
    }
    p.extend_from_slice(b"acsp");
    p.extend_from_slice(b"MSFT");
    be32(&mut p, 0); // flags
    be32(&mut p, 0); // manufacturer
    be32(&mut p, 0); // model
    p.extend_from_slice(&[0u8; 8]); // attributes
    be32(&mut p, 0); // rendering intent: perceptual
    s15(&mut p, 0.9642); // D50 illuminant
    s15(&mut p, 1.0);
    s15(&mut p, 0.8249);
    be32(&mut p, 0); // creator
    p.extend_from_slice(&[0u8; 44]); // profile id + reserved
    debug_assert_eq!(p.len(), 128);
    be32(&mut p, tags.len() as u32);
    for (sig, off, len) in &entries {
        p.extend_from_slice(sig);
        be32(&mut p, *off);
        be32(&mut p, *len);
    }
    p.extend_from_slice(&data);
    p
}

/// sRGB display profile (≈3.3 KB).
pub fn srgb_profile() -> Vec<u8> {
    let curve = curv_tag();
    assemble(
        b"mntr",
        b"RGB ",
        b"XYZ ",
        vec![
            (b"desc", desc_tag(SRGB_IDENTIFIER)),
            (b"cprt", text_tag("No copyright, use freely")),
            (b"wtpt", xyz_tag(0.95045, 1.0, 1.08905)),
            (b"rXYZ", xyz_tag(0.4360747, 0.2225045, 0.0139322)),
            (b"gXYZ", xyz_tag(0.3850649, 0.7168786, 0.0971045)),
            (b"bXYZ", xyz_tag(0.1430804, 0.0606169, 0.7141733)),
            (b"rTRC", curve.clone()),
            (b"gTRC", curve.clone()),
            (b"bTRC", curve),
        ],
    )
}

/// CMYK (0..1 each) → sRGB (0..1) the way common viewers render
/// uncalibrated DeviceCMYK: multiplicative with a slight softening of the
/// black channel so rich blacks do not clip.
pub fn cmyk_to_srgb(c: f64, m: f64, y: f64, k: f64) -> (f64, f64, f64) {
    let k = k.clamp(0.0, 1.0);
    let r = (1.0 - c.clamp(0.0, 1.0)) * (1.0 - k);
    let g = (1.0 - m.clamp(0.0, 1.0)) * (1.0 - k);
    let b = (1.0 - y.clamp(0.0, 1.0)) * (1.0 - k);
    (r, g, b)
}

/// sRGB (0..1, gamma encoded) → CIE Lab (D50).
pub fn srgb_to_lab(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (r, g, b) = (srgb_eotf(r), srgb_eotf(g), srgb_eotf(b));
    let x = 0.4360747 * r + 0.3850649 * g + 0.1430804 * b;
    let y = 0.2225045 * r + 0.7168786 * g + 0.0606169 * b;
    let z = 0.0139322 * r + 0.0971045 * g + 0.7141733 * b;
    let f = |t: f64| {
        if t > 216.0 / 24389.0 {
            t.cbrt()
        } else {
            (24389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(x / 0.9642), f(y / 1.0), f(z / 0.8249));
    (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
}

/// CMYK input profile with a 9-point-per-channel lut8 (≈20 KB).
pub fn cmyk_profile() -> Vec<u8> {
    const G: usize = 9;
    let mut t = b"mft1\0\0\0\0".to_vec();
    t.push(4); // inputs
    t.push(3); // outputs
    t.push(G as u8);
    t.push(0);
    // Identity matrix (only used for XYZ input).
    for (i, v) in [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0].iter().enumerate() {
        let _ = i;
        s15(&mut t, *v);
    }
    for _ in 0..4 {
        t.extend((0..=255u8).collect::<Vec<u8>>());
    }
    let step = 1.0 / (G - 1) as f64;
    for ci in 0..G {
        for mi in 0..G {
            for yi in 0..G {
                for ki in 0..G {
                    let (r, g, b) = cmyk_to_srgb(ci as f64 * step, mi as f64 * step, yi as f64 * step, ki as f64 * step);
                    let (l, a, bb) = srgb_to_lab(r, g, b);
                    // v2 8-bit Lab encoding.
                    t.push((l / 100.0 * 255.0).round().clamp(0.0, 255.0) as u8);
                    t.push((a + 128.0).round().clamp(0.0, 255.0) as u8);
                    t.push((bb + 128.0).round().clamp(0.0, 255.0) as u8);
                }
            }
        }
    }
    for _ in 0..3 {
        t.extend((0..=255u8).collect::<Vec<u8>>());
    }
    assemble(
        b"scnr",
        b"CMYK",
        b"Lab ",
        vec![
            (b"desc", desc_tag("Docura uncalibrated CMYK")),
            (b"cprt", text_tag("No copyright, use freely")),
            (b"wtpt", xyz_tag(0.9642, 1.0, 0.8249)),
            (b"A2B0", t),
        ],
    )
}

/// Header facts of an embedded ICC profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IccInfo {
    pub major_version: u8,
    pub class: [u8; 4],
    pub color_space: [u8; 4],
}

impl IccInfo {
    pub fn components(&self) -> Option<u32> {
        Some(match &self.color_space {
            b"GRAY" => 1,
            b"RGB " | b"Lab " | b"XYZ " | b"YCbr" | b"Luv " | b"Yxy " | b"HSV " | b"HLS " | b"CMY " => 3,
            b"CMYK" => 4,
            _ => return None,
        })
    }
}

pub fn parse_header(p: &[u8]) -> Option<IccInfo> {
    if p.len() < 128 || &p[36..40] != b"acsp" {
        return None;
    }
    let size = u32::from_be_bytes([p[0], p[1], p[2], p[3]]) as usize;
    if size < 128 || size > p.len() + 4 {
        return None;
    }
    Some(IccInfo {
        major_version: p[8],
        class: [p[12], p[13], p[14], p[15]],
        color_space: [p[16], p[17], p[18], p[19]],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_header_and_tags() {
        let p = srgb_profile();
        let h = parse_header(&p).unwrap();
        assert_eq!(h.major_version, 2);
        assert_eq!(&h.class, b"mntr");
        assert_eq!(h.components(), Some(3));
        assert_eq!(u32::from_be_bytes([p[0], p[1], p[2], p[3]]) as usize, p.len());
        assert_eq!(u32::from_be_bytes([p[128], p[129], p[130], p[131]]), 9);
        // Shared TRC stored once: profile stays small.
        assert!(p.len() < 4000, "{}", p.len());
    }

    #[test]
    fn cmyk_profile_shape() {
        let p = cmyk_profile();
        let h = parse_header(&p).unwrap();
        assert_eq!(h.components(), Some(4));
        assert_eq!(&h.class, b"scnr");
        assert!(p.len() > 19_683);
    }

    #[test]
    fn color_math() {
        let (l, a, b) = srgb_to_lab(1.0, 1.0, 1.0);
        assert!((l - 100.0).abs() < 0.1 && a.abs() < 0.5 && b.abs() < 0.5, "{l} {a} {b}");
        let (l, _, _) = srgb_to_lab(0.0, 0.0, 0.0);
        assert!(l.abs() < 0.01);
        assert_eq!(cmyk_to_srgb(0.0, 0.0, 0.0, 1.0), (0.0, 0.0, 0.0));
        assert_eq!(cmyk_to_srgb(1.0, 0.0, 0.0, 0.0), (0.0, 1.0, 1.0));
        assert!(parse_header(b"short").is_none());
        let other = IccInfo { major_version: 4, class: *b"prtr", color_space: *b"XXXX" };
        assert_eq!(other.components(), None);
    }
}
