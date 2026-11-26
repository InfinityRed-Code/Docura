//! PDF/A parts and conformance levels.

use std::fmt;
use std::str::FromStr;

/// Conformance level within a part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Conformance {
    /// Level A (accessible): logical structure + Unicode (parts 1–3).
    A,
    /// Level B (basic): visual appearance (parts 1–3).
    B,
    /// Level U (Unicode): B + Unicode text (parts 2, 3).
    U,
    /// Part 4 base level (no conformance letter).
    Base,
    /// Part 4e: engineering (3D / RichMedia allowed).
    E,
    /// Part 4f: arbitrary embedded files (at least one required).
    F,
}

/// A PDF/A level such as 2b or 4f.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Level {
    part: u8,
    conformance: Conformance,
}

/// Every level the converter produces, in UI order.
pub const ALL_LEVELS: [Level; 11] = [
    Level { part: 1, conformance: Conformance::A },
    Level { part: 1, conformance: Conformance::B },
    Level { part: 2, conformance: Conformance::A },
    Level { part: 2, conformance: Conformance::B },
    Level { part: 2, conformance: Conformance::U },
    Level { part: 3, conformance: Conformance::A },
    Level { part: 3, conformance: Conformance::B },
    Level { part: 3, conformance: Conformance::U },
    Level { part: 4, conformance: Conformance::Base },
    Level { part: 4, conformance: Conformance::E },
    Level { part: 4, conformance: Conformance::F },
];

/// What the converter does with embedded files at a level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentPolicy {
    /// No embedded files at all (part 1).
    Forbidden,
    /// Only PDF/A files (part 2: PDF/A-1/-2, part 4: PDF/A-1/-2/-4). Embedded
    /// PDFs are converted, anything else is removed.
    PdfaOnly,
    /// Any file, associated through /AF (part 3, 4f).
    Any,
}

impl Level {
    pub const fn new(part: u8, conformance: Conformance) -> Option<Level> {
        let ok = match part {
            1 => matches!(conformance, Conformance::A | Conformance::B),
            2 | 3 => matches!(conformance, Conformance::A | Conformance::B | Conformance::U),
            4 => matches!(conformance, Conformance::Base | Conformance::E | Conformance::F),
            _ => false,
        };
        if ok {
            Some(Level { part, conformance })
        } else {
            None
        }
    }

    pub fn part(self) -> u8 {
        self.part
    }

    pub fn conformance(self) -> Conformance {
        self.conformance
    }

    /// Short id as used in settings and file names: "1b", "2u", "4", "4f".
    pub fn id(self) -> String {
        let letter = match self.conformance {
            Conformance::A => "a",
            Conformance::B => "b",
            Conformance::U => "u",
            Conformance::Base => "",
            Conformance::E => "e",
            Conformance::F => "f",
        };
        format!("{}{}", self.part, letter)
    }

    /// Display label, e.g. "PDF/A-2b".
    pub fn label(self) -> String {
        format!("PDF/A-{}", self.id())
    }

    /// Value of `pdfaid:conformance`, if the level has one.
    pub fn pdfaid_conformance(self) -> Option<&'static str> {
        match self.conformance {
            Conformance::A => Some("A"),
            Conformance::B => Some("B"),
            Conformance::U => Some("U"),
            Conformance::Base => None,
            Conformance::E => Some("E"),
            Conformance::F => Some("F"),
        }
    }

    /// Header version written to the file.
    pub fn pdf_version(self) -> &'static str {
        match self.part {
            1 => "1.4",
            2 | 3 => "1.7",
            _ => "2.0",
        }
    }

    /// Every used glyph must map to Unicode (levels a and u; 1a via 6.3.8).
    pub fn needs_unicode(self) -> bool {
        matches!(self.conformance, Conformance::A | Conformance::U)
    }

    /// Logical structure (tagging) required.
    pub fn needs_structure(self) -> bool {
        self.conformance == Conformance::A
    }

    pub fn allows_transparency(self) -> bool {
        self.part >= 2
    }

    pub fn allows_jpx(self) -> bool {
        self.part >= 2
    }

    pub fn allows_optional_content(self) -> bool {
        self.part >= 2
    }

    /// Object streams / cross-reference streams (PDF 1.5+).
    pub fn allows_object_streams(self) -> bool {
        self.part >= 2
    }

    /// The document information dictionary is kept (parts 1–3) or dropped (4).
    pub fn keeps_info_dict(self) -> bool {
        self.part <= 3
    }

    pub fn attachments(self) -> AttachmentPolicy {
        match (self.part, self.conformance) {
            (1, _) => AttachmentPolicy::Forbidden,
            (3, _) | (4, Conformance::F) => AttachmentPolicy::Any,
            _ => AttachmentPolicy::PdfaOnly,
        }
    }

    /// The level used for embedded PDFs under [`AttachmentPolicy::PdfaOnly`].
    pub fn attachment_level(self) -> Level {
        match self.part {
            4 => Level { part: 4, conformance: Conformance::Base },
            _ => Level { part: 2, conformance: Conformance::B },
        }
    }

    /// 3D and RichMedia annotations survive (4e only).
    pub fn allows_3d(self) -> bool {
        self.part == 4 && self.conformance == Conformance::E
    }

    /// Page boundaries are limited to 3 … 14400 units (required by parts 2
    /// and 3; part 1 inherits the same limit from PDF 1.4 viewers and would
    /// otherwise hit its limit for real numbers).
    pub fn limits_page_size(self) -> bool {
        self.part <= 3
    }

    /// Largest number of DeviceN colorants.
    pub fn max_devicen_components(self) -> usize {
        if self.part == 1 {
            8
        } else {
            32
        }
    }

    /// PDF/A-1 implementation limits are stricter.
    pub fn max_real(self) -> f64 {
        if self.part == 1 {
            32767.0
        } else {
            3.403e38
        }
    }

    pub fn max_string_len(self) -> usize {
        if self.part == 1 {
            65535
        } else {
            32767
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label())
    }
}

impl FromStr for Level {
    type Err = String;

    /// Accepts "2b", "2B", "PDF/A-2b", "pdfa-2b"; a bare part number means
    /// the b level ("2" → 2b) except for part 4 (base level).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = s.trim().to_ascii_lowercase();
        let t = t
            .strip_prefix("pdf/a-")
            .or_else(|| t.strip_prefix("pdfa-"))
            .or_else(|| t.strip_prefix("pdfa"))
            .unwrap_or(&t);
        let mut chars = t.chars();
        let part = chars
            .next()
            .and_then(|c| c.to_digit(10))
            .ok_or_else(|| format!("unsupported PDF/A level: {s}"))? as u8;
        let rest: String = chars.collect();
        let conformance = match (part, rest.as_str()) {
            (4, "") => Conformance::Base,
            (_, "") => Conformance::B,
            (_, "a") => Conformance::A,
            (_, "b") => Conformance::B,
            (_, "u") => Conformance::U,
            (_, "e") => Conformance::E,
            (_, "f") => Conformance::F,
            _ => return Err(format!("unsupported PDF/A level: {s}")),
        };
        Level::new(part, conformance).ok_or_else(|| format!("unsupported PDF/A level: {s}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_all_ids_roundtrip() {
        for l in ALL_LEVELS {
            assert_eq!(l.id().parse::<Level>().unwrap(), l);
        }
        assert_eq!("2".parse::<Level>().unwrap().id(), "2b");
        assert_eq!("PDF/A-3U".parse::<Level>().unwrap().id(), "3u");
        assert_eq!("4".parse::<Level>().unwrap().id(), "4");
        assert!("1u".parse::<Level>().is_err());
        assert!("4b".parse::<Level>().is_err());
        assert!("5".parse::<Level>().is_err());
        assert!("x".parse::<Level>().is_err());
        assert!("2z".parse::<Level>().is_err());
    }

    #[test]
    fn capabilities() {
        let l1b: Level = "1b".parse().unwrap();
        let l2u: Level = "2u".parse().unwrap();
        let l3a: Level = "3a".parse().unwrap();
        let l4: Level = "4".parse().unwrap();
        let l4f: Level = "4f".parse().unwrap();
        let l4e: Level = "4e".parse().unwrap();
        assert_eq!(l1b.pdf_version(), "1.4");
        assert_eq!(l2u.pdf_version(), "1.7");
        assert_eq!(l4.pdf_version(), "2.0");
        assert!(!l1b.allows_transparency() && l2u.allows_transparency());
        assert!(l2u.needs_unicode() && !l2u.needs_structure());
        assert!(l3a.needs_structure() && l3a.needs_unicode());
        assert_eq!(l4.pdfaid_conformance(), None);
        assert_eq!(l4f.pdfaid_conformance(), Some("F"));
        assert_eq!(l1b.attachments(), AttachmentPolicy::Forbidden);
        assert_eq!(l2u.attachments(), AttachmentPolicy::PdfaOnly);
        assert_eq!(l3a.attachments(), AttachmentPolicy::Any);
        assert_eq!(l4.attachments(), AttachmentPolicy::PdfaOnly);
        assert_eq!(l4f.attachments(), AttachmentPolicy::Any);
        assert_eq!(l4.attachment_level().id(), "4");
        assert_eq!(l2u.attachment_level().id(), "2b");
        assert!(l4e.allows_3d() && !l4.allows_3d());
        assert!(!l4.keeps_info_dict() && l3a.keeps_info_dict());
        assert_eq!(l1b.max_real(), 32767.0);
        assert_eq!(l1b.max_string_len(), 65535);
        assert_eq!(l2u.max_string_len(), 32767);
        assert_eq!(l2u.to_string(), "PDF/A-2u");
        assert!(l2u.allows_jpx() && !l1b.allows_jpx());
        assert!(l2u.allows_optional_content() && !l1b.allows_optional_content());
        assert!(l2u.allows_object_streams() && !l1b.allows_object_streams());
        assert_eq!(ALL_LEVELS.len(), 11);
    }
}
