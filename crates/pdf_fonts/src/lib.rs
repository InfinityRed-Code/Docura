//! Open-licensed fonts shipped with the app.
//!
//! The PDF standard fonts (Helvetica, Times, Courier) need no file. Every
//! family listed here is an unmodified static TrueType font under the SIL Open
//! Font License 1.1, compiled into the binary so the text editor (UI preview),
//! the FreeText writer (`pdf_mutate`) and the page-text editor (`pdf_pdfium`)
//! all use the very same bytes. Licenses and sources: `fonts/README.md`.

use serde::Serialize;

/// Broad look of a family, used to group the font pickers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Sans,
    Serif,
    Mono,
    Handwriting,
}

/// One bundled font family.
#[derive(Debug, Serialize)]
pub struct Family {
    /// Stable id written into documents ("Roboto"). Never one of the
    /// standard-font ids "Helv", "Times", "Cour".
    pub id: &'static str,
    /// Display name ("Open Sans").
    pub name: &'static str,
    pub category: Category,
    /// Whether the family has real bold / italic faces. Missing faces fall
    /// back to the closest one that exists.
    pub bold: bool,
    pub italic: bool,
    /// PostScript names of the faces are "<stem>-Regular", "<stem>-Bold", …
    #[serde(skip)]
    ps_stem: &'static str,
    #[serde(skip)]
    faces: Faces,
}

/// Regular, bold, italic, bold italic.
#[derive(Debug)]
struct Faces([Option<&'static [u8]>; 4]);

macro_rules! font {
    ($dir:literal, $stem:literal, $style:literal) => {
        Some(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fonts/",
            $dir,
            "/",
            $stem,
            "-",
            $style,
            ".ttf"
        )) as &[u8])
    };
}

macro_rules! family4 {
    ($id:literal, $name:literal, $cat:expr, $dir:literal, $stem:literal) => {
        Family {
            id: $id,
            name: $name,
            category: $cat,
            bold: true,
            italic: true,
            ps_stem: $stem,
            faces: Faces([
                font!($dir, $stem, "Regular"),
                font!($dir, $stem, "Bold"),
                font!($dir, $stem, "Italic"),
                font!($dir, $stem, "BoldItalic"),
            ]),
        }
    };
}

static FAMILIES: [Family; 8] = [
    family4!("Roboto", "Roboto", Category::Sans, "roboto", "Roboto"),
    family4!(
        "OpenSans",
        "Open Sans",
        Category::Sans,
        "opensans",
        "OpenSans"
    ),
    family4!("Lato", "Lato", Category::Sans, "lato", "Lato"),
    family4!(
        "Montserrat",
        "Montserrat",
        Category::Sans,
        "montserrat",
        "Montserrat"
    ),
    family4!("Lora", "Lora", Category::Serif, "lora", "Lora"),
    family4!(
        "SourceSerif4",
        "Source Serif 4",
        Category::Serif,
        "sourceserif4",
        "SourceSerif4"
    ),
    family4!(
        "RobotoMono",
        "Roboto Mono",
        Category::Mono,
        "robotomono",
        "RobotoMono"
    ),
    Family {
        id: "Caveat",
        name: "Caveat",
        category: Category::Handwriting,
        bold: true,
        italic: false,
        ps_stem: "Caveat",
        faces: Faces([
            font!("caveat", "Caveat", "Regular"),
            font!("caveat", "Caveat", "Bold"),
            None,
            None,
        ]),
    },
];

/// Every bundled family, in display order.
pub fn families() -> &'static [Family] {
    &FAMILIES
}

/// The bundled family with this id, if any.
pub fn family(id: &str) -> Option<&'static Family> {
    FAMILIES.iter().find(|f| f.id == id)
}

/// True for the ids of the bundled families (as opposed to the standard
/// fonts "Helv" / "Times" / "Cour").
pub fn is_bundled(id: &str) -> bool {
    family(id).is_some()
}

/// The bundled face a PDF font name refers to, as (family, bold, italic):
/// "Roboto-BoldItalic", also with a subset prefix ("ABCDEF+Roboto-Bold").
pub fn by_postscript_name(name: &str) -> Option<(&'static Family, bool, bool)> {
    let name = name.split_once('+').map_or(name, |(_, n)| n);
    let (stem, style) = name.split_once('-')?;
    let family = FAMILIES.iter().find(|f| f.ps_stem == stem)?;
    let (bold, italic) = match style {
        "Regular" => (false, false),
        "Bold" => (true, false),
        "Italic" => (false, true),
        "BoldItalic" => (true, true),
        _ => return None,
    };
    Some((family, bold, italic))
}

impl Family {
    /// TrueType bytes of the face closest to the requested style: a missing
    /// italic falls back to upright, a missing bold to regular.
    pub fn face(&self, bold: bool, italic: bool) -> &'static [u8] {
        let idx = |b: bool, i: bool| (b as usize) | ((i as usize) << 1);
        let f = &self.faces.0;
        f[idx(bold, italic)]
            .or(f[idx(bold, false)])
            .or(f[idx(false, italic)])
            .or(f[0])
            .expect("every family has a regular face")
    }

    /// The style a request for `bold` / `italic` actually gets.
    pub fn effective_style(&self, bold: bool, italic: bool) -> (bool, bool) {
        (bold && self.bold, italic && self.italic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_never_standard_fonts() {
        let mut seen = std::collections::HashSet::new();
        for f in families() {
            assert!(seen.insert(f.id), "duplicate id {}", f.id);
            assert!(!matches!(f.id, "Helv" | "Times" | "Cour"));
            assert!(
                f.id.bytes().all(|b| b.is_ascii_alphanumeric()),
                "id must be a PDF-name-safe token"
            );
        }
        assert!(is_bundled("Roboto") && !is_bundled("Helv"));
    }

    #[test]
    fn every_face_parses_and_covers_german_text() {
        for f in families() {
            for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                let face = ttf_parser::Face::parse(f.face(bold, italic), 0)
                    .unwrap_or_else(|e| panic!("{} {bold}/{italic}: {e}", f.id));
                // Static TrueType outlines (glyf), no variation axes: what
                // PDFium and the FreeText writer can embed.
                assert!(!face.is_variable(), "{} is variable", f.id);
                assert!(face.tables().glyf.is_some(), "{} has no glyf table", f.id);
                for c in "AZaz09ÄÖÜäöüß€„“–".chars() {
                    assert!(face.glyph_index(c).is_some(), "{} lacks {c:?}", f.id);
                }
                // fsType 0 = installable embedding (OFL fonts): may be
                // embedded in documents without restriction.
                assert!(
                    face.permissions().is_none()
                        || matches!(
                            face.permissions(),
                            Some(ttf_parser::Permissions::Installable)
                        ),
                    "{} restricts embedding",
                    f.id
                );
            }
        }
    }

    #[test]
    fn postscript_names_map_back_to_faces() {
        for f in families() {
            for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                let (bold, italic) = f.effective_style(bold, italic);
                let face = ttf_parser::Face::parse(f.face(bold, italic), 0).unwrap();
                let ps = face
                    .names()
                    .into_iter()
                    .filter(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
                    .find_map(|n| n.to_string())
                    .unwrap();
                let (found, b, i) = by_postscript_name(&format!("ABCDEF+{ps}")).unwrap();
                assert_eq!((found.id, b, i), (f.id, bold, italic), "{ps}");
            }
        }
        assert!(by_postscript_name("Arial-BoldMT").is_none());
        assert!(by_postscript_name("Helvetica").is_none());
    }

    #[test]
    fn missing_faces_fall_back() {
        let caveat = family("Caveat").unwrap();
        assert_eq!(
            caveat.face(false, true).as_ptr(),
            caveat.face(false, false).as_ptr()
        );
        assert_eq!(
            caveat.face(true, true).as_ptr(),
            caveat.face(true, false).as_ptr()
        );
        assert_eq!(caveat.effective_style(true, true), (true, false));
    }
}
