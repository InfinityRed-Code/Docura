//! System font lookup for fonts the PDF references but does not embed.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Debug)]
pub struct SysFont {
    pub path: PathBuf,
    pub index: u32,
    pub ps_name: String,
    pub family: String,
    pub full: String,
    pub bold: bool,
    pub italic: bool,
}

/// Font bytes plus face index.
#[derive(Clone)]
pub struct FontData {
    pub data: Arc<Vec<u8>>,
    pub index: u32,
    /// Name used for /BaseFont.
    pub ps_name: String,
}

pub fn default_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if cfg!(windows) {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        v.push(win.join("Fonts"));
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            v.push(PathBuf::from(l).join("Microsoft").join("Windows").join("Fonts"));
        }
    } else if cfg!(target_os = "macos") {
        v.push(PathBuf::from("/System/Library/Fonts"));
        v.push(PathBuf::from("/Library/Fonts"));
        if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(h).join("Library/Fonts"));
        }
    } else {
        v.push(PathBuf::from("/usr/share/fonts"));
        v.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(&h).join(".fonts"));
            v.push(PathBuf::from(h).join(".local/share/fonts"));
        }
    }
    v
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase()
}

fn read_at(f: &mut std::fs::File, off: u64, len: usize) -> Option<Vec<u8>> {
    f.seek(SeekFrom::Start(off)).ok()?;
    let mut b = vec![0u8; len];
    f.read_exact(&mut b).ok()?;
    Some(b)
}

fn be16(b: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([b[o], b[o + 1]])
}

fn be32(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Names of one face; None for CFF-only or restricted fonts.
fn read_face(f: &mut std::fs::File, off: u64) -> Option<(String, String, String, String)> {
    let hdr = read_at(f, off, 12)?;
    let n = be16(&hdr, 4) as usize;
    let dir = read_at(f, off + 12, 16 * n)?;
    let mut name_tbl = None;
    let mut os2 = None;
    let mut has_glyf = false;
    for i in 0..n {
        let r = &dir[16 * i..16 * i + 16];
        match &r[0..4] {
            b"name" => name_tbl = Some((be32(r, 8) as u64, be32(r, 12) as usize)),
            b"OS/2" => os2 = Some(be32(r, 8) as u64),
            b"glyf" => has_glyf = true,
            _ => {}
        }
    }
    if !has_glyf {
        return None;
    }
    if let Some(o) = os2 {
        let b = read_at(f, o + 8, 2)?;
        let fs_type = be16(&b, 0);
        // Restricted license embedding.
        if fs_type & 0x000F == 0x0002 {
            return None;
        }
    }
    let (no, nl) = name_tbl?;
    if nl > 1 << 20 {
        return None;
    }
    let t = read_at(f, no, nl)?;
    if t.len() < 6 {
        return None;
    }
    let count = be16(&t, 2) as usize;
    let sto = be16(&t, 4) as usize;
    let mut names: HashMap<u16, String> = HashMap::new();
    for i in 0..count {
        let r = 6 + 12 * i;
        if r + 12 > t.len() {
            break;
        }
        let (pid, eid, lid, nid, len, o) = (be16(&t, r), be16(&t, r + 2), be16(&t, r + 4), be16(&t, r + 6), be16(&t, r + 8) as usize, be16(&t, r + 10) as usize);
        let Some(bytes) = t.get(sto + o..sto + o + len) else { continue };
        let s = match (pid, eid) {
            (3, 1) | (3, 10) | (0, _) => {
                if pid == 3 && lid != 0x409 && names.contains_key(&nid) {
                    continue;
                }
                let u: Vec<u16> = bytes.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                String::from_utf16_lossy(&u)
            }
            (1, 0) => {
                if names.contains_key(&nid) {
                    continue;
                }
                bytes.iter().map(|&b| b as char).collect()
            }
            _ => continue,
        };
        names.insert(nid, s);
    }
    let family = names.get(&16).or(names.get(&1)).cloned().unwrap_or_default();
    let sub = names.get(&17).or(names.get(&2)).cloned().unwrap_or_default();
    let full = names.get(&4).cloned().unwrap_or_default();
    let ps = names.get(&6).cloned().unwrap_or_default();
    Some((ps, family, sub, full))
}

fn scan_file(path: &Path, out: &mut Vec<SysFont>) {
    let Ok(mut f) = std::fs::File::open(path) else { return };
    let Some(h) = read_at(&mut f, 0, 12) else { return };
    let offsets: Vec<u64> = if &h[0..4] == b"ttcf" {
        let n = be32(&h, 8).min(64) as usize;
        match read_at(&mut f, 12, 4 * n) {
            Some(b) => (0..n).map(|i| be32(&b, 4 * i) as u64).collect(),
            None => return,
        }
    } else {
        vec![0]
    };
    for (i, off) in offsets.into_iter().enumerate() {
        if let Some((ps, family, sub, full)) = read_face(&mut f, off) {
            let s = sub.to_ascii_lowercase();
            out.push(SysFont {
                path: path.to_path_buf(),
                index: i as u32,
                bold: s.contains("bold") || s.contains("black") || s.contains("heavy"),
                italic: s.contains("italic") || s.contains("oblique"),
                ps_name: ps,
                family,
                full,
            });
        }
    }
}

fn scan_dir(dir: &Path, out: &mut Vec<SysFont>, depth: u32) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth < 4 {
                scan_dir(&p, out, depth + 1);
            }
            continue;
        }
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if matches!(ext.as_str(), "ttf" | "ttc") {
            scan_file(&p, out);
        }
    }
}

type Index = Arc<Vec<SysFont>>;

fn index_for(dirs: &[PathBuf]) -> Index {
    static CACHE: OnceLock<Mutex<HashMap<Vec<PathBuf>, Index>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key: Vec<PathBuf> = if dirs.is_empty() { default_dirs() } else { dirs.to_vec() };
    if let Some(i) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return i;
    }
    let mut v = Vec::new();
    for d in &key {
        scan_dir(d, &mut v, 0);
    }
    let idx = Arc::new(v);
    if let Ok(mut c) = cache.lock() {
        c.insert(key, idx.clone());
    }
    idx
}

fn load(f: &SysFont) -> Option<FontData> {
    static FILES: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<u8>>>>> = OnceLock::new();
    let files = FILES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(d) = files.lock().ok().and_then(|m| m.get(&f.path).cloned()) {
        return Some(FontData { data: d, index: f.index, ps_name: f.ps_name.clone() });
    }
    let data = Arc::new(std::fs::read(&f.path).ok()?);
    if let Ok(mut m) = files.lock() {
        if m.len() > 32 {
            m.clear();
        }
        m.insert(f.path.clone(), data.clone());
    }
    Some(FontData { data, index: f.index, ps_name: f.ps_name.clone() })
}

/// What we know about the wanted font.
#[derive(Clone, Debug, Default)]
pub struct Want {
    pub name: String,
    pub bold: bool,
    pub italic: bool,
    pub serif: bool,
    pub fixed: bool,
}

/// Families to try for a standard-14 or generic request, best first.
fn generic_families(w: &Want, std: Option<&str>) -> Vec<&'static str> {
    match std.unwrap_or("") {
        s if s.starts_with("Times") => vec!["Times New Roman", "Liberation Serif", "Tinos", "DejaVu Serif", "Noto Serif"],
        s if s.starts_with("Courier") => vec!["Courier New", "Liberation Mono", "Cousine", "DejaVu Sans Mono", "Noto Sans Mono"],
        s if s.starts_with("Helvetica") => vec!["Arial", "Liberation Sans", "Arimo", "Helvetica", "DejaVu Sans", "Noto Sans"],
        "Symbol" => vec!["Symbol", "Standard Symbols PS", "Standard Symbols L"],
        "ZapfDingbats" => vec!["Segoe UI Symbol", "DejaVu Sans", "Noto Sans Symbols2", "Noto Sans Symbols"],
        _ => {
            if w.fixed {
                vec!["Courier New", "Liberation Mono", "DejaVu Sans Mono"]
            } else if w.serif {
                vec!["Times New Roman", "Liberation Serif", "DejaVu Serif"]
            } else {
                vec!["Arial", "Liberation Sans", "DejaVu Sans", "Noto Sans"]
            }
        }
    }
}

fn pick<'a>(cands: impl Iterator<Item = &'a SysFont>, bold: bool, italic: bool) -> Option<&'a SysFont> {
    let v: Vec<&SysFont> = cands.collect();
    v.iter()
        .find(|f| f.bold == bold && f.italic == italic)
        .or_else(|| v.iter().find(|f| f.bold == bold))
        .or_else(|| v.iter().find(|f| !f.bold && !f.italic))
        .or(v.first())
        .copied()
}

/// Find the font by name in the system; `exact_only` stops before the
/// generic fallbacks.
pub fn find(dirs: &[PathBuf], w: &Want, std: Option<&str>, exact_only: bool) -> Option<FontData> {
    let idx = index_for(dirs);
    let base = super::encoding::strip_subset(&w.name);
    let n = norm(base);
    if !n.is_empty() {
        if let Some(f) = idx.iter().find(|f| norm(&f.ps_name) == n) {
            return load(f);
        }
        if let Some(f) = idx.iter().find(|f| norm(&f.full) == n) {
            return load(f);
        }
        // "Family,Style" / "Family-Style".
        let fam = base.split([',', '-']).next().unwrap_or(base);
        let nf = norm(fam.trim_end_matches("MT").trim_end_matches("PS"));
        if let Some(f) = pick(idx.iter().filter(|f| norm(&f.family) == nf), w.bold, w.italic) {
            if std.is_none() {
                return load(f);
            }
        }
    }
    if exact_only {
        return None;
    }
    for fam in generic_families(w, std) {
        let nf = norm(fam);
        if let Some(f) = pick(idx.iter().filter(|f| norm(&f.family) == nf), w.bold, w.italic) {
            return load(f);
        }
    }
    None
}

/// Bundled fallback (always available).
pub fn bundled(w: &Want) -> FontData {
    let fam = if w.fixed { "RobotoMono" } else if w.serif { "SourceSerif4" } else { "Roboto" };
    let f = pdf_fonts::family(fam).or_else(|| pdf_fonts::family("Roboto")).expect("bundled fonts");
    let data = f.face(w.bold, w.italic);
    static CACHE: OnceLock<Mutex<HashMap<(usize, usize), Arc<Vec<u8>>>>> = OnceLock::new();
    let key = (data.as_ptr() as usize, data.len());
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let arc = cache
        .lock()
        .ok()
        .and_then(|c| c.get(&key).cloned())
        .unwrap_or_else(|| {
            let a = Arc::new(data.to_vec());
            if let Ok(mut c) = cache.lock() {
                c.insert(key, a.clone());
            }
            a
        });
    let ps = ttf_parser::Face::parse(&arc, 0)
        .ok()
        .and_then(|face| {
            face.names()
                .into_iter()
                .find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
                .and_then(|n| n.to_string())
        })
        .unwrap_or_else(|| fam.to_string());
    FontData { data: arc, index: 0, ps_name: ps }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_fallback() {
        let f = bundled(&Want { serif: true, ..Default::default() });
        assert!(f.ps_name.contains("Serif"));
        let m = bundled(&Want { fixed: true, bold: true, ..Default::default() });
        assert!(m.ps_name.contains("Mono"));
        assert!(!bundled(&Want::default()).data.is_empty());
    }

    #[test]
    fn scans_a_directory() {
        // The bundled font directory doubles as a tiny "system" font dir.
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("pdf_fonts").join("fonts");
        let w = Want { name: "Roboto-Bold".into(), ..Default::default() };
        let f = find(&[dir.clone()], &w, None, true).expect("Roboto-Bold by PostScript name");
        assert_eq!(f.ps_name, "Roboto-Bold");
        let w2 = Want { name: "Lato,Italic".into(), italic: true, ..Default::default() };
        let f2 = find(&[dir.clone()], &w2, None, true).expect("Lato italic by family");
        assert!(f2.ps_name.contains("Italic"), "{}", f2.ps_name);
        assert!(find(&[dir.clone()], &Want { name: "NoSuchFont".into(), ..Default::default() }, None, true).is_none());
        assert_eq!(norm("Times New Roman"), "timesnewroman");
        assert!(!generic_families(&Want { fixed: true, ..Default::default() }, None).is_empty());
    }
}
