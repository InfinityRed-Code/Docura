//! Corpus files for the less common shapes of fonts, annotations, colour
//! and attachments: the things real producers write that the first corpus
//! did not reach.

use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};

use super::{font_res, helv, minimal_cff, B, TEXT};

fn name(s: &str) -> Object {
    Object::Name(s.as_bytes().to_vec())
}

fn rect(a: i64, b: i64, c: i64, d: i64) -> Object {
    vec![a.into(), b.into(), c.into(), d.into()].into()
}

fn form(b: &mut B, w: i64, h: i64, content: &[u8], extra: Dictionary) -> ObjectId {
    let mut d = dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => rect(0, 0, w, h) };
    for (k, v) in extra.iter() {
        d.set(k.clone(), v.clone());
    }
    b.doc.add_object(Stream::new(d, content.to_vec()))
}

fn roboto() -> Vec<u8> {
    pdf_fonts::family("Roboto").unwrap().face(false, false).to_vec()
}

fn roboto_file(b: &mut B) -> ObjectId {
    let data = roboto();
    let mut ff = Stream::new(dictionary! { "Length1" => data.len() as i64 }, data);
    let _ = ff.compress();
    b.doc.add_object(ff)
}

fn descriptor(b: &mut B, font_name: &str, key: &str, file: ObjectId, flags: i64) -> ObjectId {
    let mut fd = dictionary! {
        "Type" => "FontDescriptor", "FontName" => name(font_name), "Flags" => flags,
        "FontBBox" => rect(-700, -300, 2000, 1000),
        "ItalicAngle" => 0, "Ascent" => 928, "Descent" => (-244), "CapHeight" => 711, "StemV" => 80,
    };
    fd.set(key, file);
    b.doc.add_object(fd)
}

/// Glyph ids and advances (1/1000 em) of `text` in Roboto.
fn roboto_glyphs(text: &str) -> Vec<(u16, i64)> {
    let data = roboto();
    let face = ttf_parser::Face::parse(&data, 0).unwrap();
    let upem = face.units_per_em() as f64;
    text.chars()
        .map(|c| {
            let g = face.glyph_index(c).unwrap();
            (g.0, (face.glyph_hor_advance(g).unwrap() as f64 * 1000.0 / upem).round() as i64)
        })
        .collect()
}

fn hex_text(glyphs: &[(u16, i64)], y: i64) -> Vec<u8> {
    let mut c = format!("BT /F1 18 Tf 72 {y} Td <").into_bytes();
    for (g, _) in glyphs {
        c.extend_from_slice(format!("{g:04X}").as_bytes());
    }
    c.extend_from_slice(b"> Tj ET\n");
    c
}

/// A composite font over the embedded Roboto. `wrong_widths` breaks /W,
/// `map` adds a CIDToGIDMap stream (identity), `encoding` replaces
/// Identity-H.
fn roboto_type0(b: &mut B, font_name: &str, glyphs: &[(u16, i64)], wrong_widths: bool, map: bool, encoding: Option<Object>) -> ObjectId {
    let ff = roboto_file(b);
    let fd = descriptor(b, font_name, "FontFile2", ff, 4);
    let mut w = Vec::new();
    for (g, adv) in glyphs {
        w.push(Object::Integer(*g as i64));
        w.push(Object::Array(vec![(if wrong_widths { adv + 150 } else { *adv }).into()]));
    }
    let mut cf = dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => name(font_name),
        "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
        "FontDescriptor" => fd, "W" => w,
    };
    if map {
        let max = glyphs.iter().map(|g| g.0).max().unwrap_or(0);
        let bytes: Vec<u8> = (0..=max).flat_map(|g| g.to_be_bytes()).collect();
        let m = b.doc.add_object(Stream::new(Dictionary::new(), bytes));
        cf.set("CIDToGIDMap", m);
    } else {
        cf.set("CIDToGIDMap", "Identity");
    }
    let cf = b.doc.add_object(cf);
    b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => name(font_name),
        "Encoding" => encoding.unwrap_or_else(|| name("Identity-H")), "DescendantFonts" => vec![cf.into()],
    })
}

fn to_unicode(b: &mut B, pairs: &[(u32, &str)], bytes: usize) -> ObjectId {
    let mut s = String::from("/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CMapName /X def\n1 begincodespacerange ");
    s.push_str(if bytes == 1 { "<00> <FF>" } else { "<0000> <FFFF>" });
    s.push_str(&format!(" endcodespacerange\n{} beginbfchar ", pairs.len()));
    for (code, text) in pairs {
        let code = if bytes == 1 { format!("<{code:02X}>") } else { format!("<{code:04X}>") };
        let uni: String = text.encode_utf16().map(|u| format!("{u:04X}")).collect();
        s.push_str(&format!("{code} <{uni}> "));
    }
    s.push_str("endbfchar\nendcmap end end");
    b.doc.add_object(Stream::new(Dictionary::new(), s.into_bytes()))
}

fn table_checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |a, c| {
        let mut w = [0u8; 4];
        w[..c.len()].copy_from_slice(c);
        a.wrapping_add(u32::from_be_bytes(w))
    })
}

/// An OpenType font file around a bare CFF program.
pub fn opentype_with_cff(cff: &[u8], advances: &[u16]) -> Vec<u8> {
    let n = advances.len() as u16;
    let mut head = Vec::new();
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    head.extend_from_slice(&0u32.to_be_bytes());
    head.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
    head.extend_from_slice(&0u16.to_be_bytes());
    head.extend_from_slice(&1000u16.to_be_bytes());
    head.extend_from_slice(&[0; 16]);
    for v in [0i16, 0, 1000, 700] {
        head.extend_from_slice(&v.to_be_bytes());
    }
    for v in [0u16, 8, 2, 0, 0] {
        head.extend_from_slice(&v.to_be_bytes());
    }
    let mut hhea = Vec::new();
    hhea.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    for v in [700i16, 0, 0] {
        hhea.extend_from_slice(&v.to_be_bytes());
    }
    hhea.extend_from_slice(&1000u16.to_be_bytes());
    for v in [0i16, 0, 1000, 1, 0, 0, 0, 0, 0, 0, 0] {
        hhea.extend_from_slice(&v.to_be_bytes());
    }
    hhea.extend_from_slice(&n.to_be_bytes());
    let mut maxp = 0x0000_5000u32.to_be_bytes().to_vec();
    maxp.extend_from_slice(&n.to_be_bytes());
    let mut hmtx = Vec::new();
    for a in advances {
        hmtx.extend_from_slice(&a.to_be_bytes());
        hmtx.extend_from_slice(&0i16.to_be_bytes());
    }
    let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![(b"CFF ", cff.to_vec()), (b"head", head), (b"hhea", hhea), (b"hmtx", hmtx), (b"maxp", maxp)];
    let mut out = b"OTTO".to_vec();
    out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
    for v in [64u16, 2, 16] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    for (tag, data) in &tables {
        out.extend_from_slice(*tag);
        out.extend_from_slice(&table_checksum(data).to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    out.extend_from_slice(&body);
    out
}

/// Embedded fonts in the shapes producers write them: encoding
/// differences, existing ToUnicode maps, composite fonts with and without
/// CIDToGIDMap, wrong /W, CID-keyed use of a CFF program, an embedded CMap,
/// OpenType-wrapped CFF, and a Type 3 font that already maps to Unicode.
pub fn edge_fonts() -> Vec<u8> {
    let mut b = B::new();

    // 1: simple TrueType with an encoding dictionary and a ToUnicode map.
    let ff = roboto_file(&mut b);
    let fd = descriptor(&mut b, "Roboto-Regular", "FontFile2", ff, 32);
    let g = roboto_glyphs("ABC");
    let tu = to_unicode(&mut b, &[(65, "B"), (66, "B"), (67, "C")], 1);
    let f = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Roboto-Regular", "FirstChar" => 65, "LastChar" => 67,
        "Widths" => vec![g[1].1.into(), g[1].1.into(), g[2].1.into()],
        "Encoding" => dictionary! { "Type" => "Encoding", "BaseEncoding" => "WinAnsiEncoding", "Differences" => vec![65.into(), name("B")] },
        "FontDescriptor" => fd, "ToUnicode" => tu,
    });
    b.page(b"BT /F1 18 Tf 72 760 Td (ABC) Tj ET", font_res(f), Dictionary::new());

    // 2: composite fonts over one program: wrong /W (rebuilt), identity map
    // with a subset name, CIDToGIDMap stream.
    let glyphs = roboto_glyphs("Komposit");
    let f_wrong = roboto_type0(&mut b, "Roboto-Regular", &glyphs, true, true, None);
    let f_ident = roboto_type0(&mut b, "ABCDEF+Roboto-Regular", &glyphs, false, false, None);
    let f_map = roboto_type0(&mut b, "GHIJKL+Roboto-Regular", &glyphs, false, true, None);
    // An embedded CMap that only refers to Identity-H.
    let cmap = b.doc.add_object(Stream::new(
        dictionary! { "Type" => "CMap", "CMapName" => "Docura-Test-H", "UseCMap" => "Identity-H", "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 } },
        b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap /Identity-H usecmap /CMapName /Docura-Test-H def endcmap end end".to_vec(),
    ));
    let f_cmap = roboto_type0(&mut b, "Roboto-Regular", &glyphs, false, false, Some(cmap.into()));
    let mut c = Vec::new();
    let res = dictionary! { "Font" => dictionary! { "F1" => f_wrong, "F2" => f_ident, "F3" => f_map, "F4" => f_cmap } };
    for (i, y) in [760, 720, 680, 640].iter().enumerate() {
        c.extend_from_slice(&String::from_utf8(hex_text(&glyphs, *y)).unwrap().replace("/F1", &format!("/F{}", i + 1)).into_bytes());
    }
    b.page(&c, res, Dictionary::new());

    // 3: a CFF program used as a CID font, with a subset name.
    let cff = minimal_cff("MNOPQR+DocuraCid", &[('A', 600), ('B', 700)]);
    let ff = b.doc.add_object(Stream::new(dictionary! { "Subtype" => "CIDFontType0C" }, cff));
    let fd = descriptor(&mut b, "MNOPQR+DocuraCid", "FontFile3", ff, 4);
    let cf = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType0", "BaseFont" => "MNOPQR+DocuraCid",
        "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
        "FontDescriptor" => fd, "DW" => 500, "W" => vec![1.into(), Object::Array(vec![600.into(), 700.into()])],
    });
    let tu = to_unicode(&mut b, &[(1, "A"), (2, "B")], 2);
    let f = b.doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "MNOPQR+DocuraCid", "Encoding" => "Identity-H", "DescendantFonts" => vec![cf.into()], "ToUnicode" => tu });
    b.page(b"BT /F1 24 Tf 72 700 Td <00010002> Tj ET", font_res(f), Dictionary::new());

    // 4: CFF inside an OpenType file (FontFile3 /OpenType, PDF 1.6).
    let cff = minimal_cff("DocuraOtf", &[('A', 600), ('B', 700)]);
    let otf = opentype_with_cff(&cff, &[500, 600, 700]);
    let ff = b.doc.add_object(Stream::new(dictionary! { "Subtype" => "OpenType" }, otf));
    let fd = descriptor(&mut b, "DocuraOtf", "FontFile3", ff, 32);
    let f = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "DocuraOtf", "FirstChar" => 65, "LastChar" => 66,
        "Widths" => vec![600.into(), 700.into()], "Encoding" => "WinAnsiEncoding", "FontDescriptor" => fd,
    });
    b.page(b"BT /F1 24 Tf 72 700 Td (ABBA) Tj ET", font_res(f), Dictionary::new());

    // 5: Type 3 font with its own ToUnicode map.
    let sq = b.doc.add_object(Stream::new(Dictionary::new(), b"1000 0 0 0 750 750 d1 0 0 750 750 re f".to_vec()));
    // (The entry for "B" is U+0000, which no level accepts.)
    let tu = to_unicode(&mut b, &[(65, "\u{25A0}"), (66, "\0")], 1);
    let f = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type3", "FontBBox" => rect(0, 0, 750, 750),
        "FontMatrix" => vec![0.001.into(), 0.into(), 0.into(), 0.001.into(), 0.into(), 0.into()],
        "CharProcs" => dictionary! { "square" => sq },
        "Encoding" => dictionary! { "Type" => "Encoding", "Differences" => vec![65.into(), name("square"), name("square")] },
        "FirstChar" => 65, "LastChar" => 66, "Widths" => vec![1000.into(), 1000.into()], "Resources" => Dictionary::new(), "ToUnicode" => tu,
    });
    b.page(b"BT /F1 24 Tf 72 700 Td (ABA) Tj ET", font_res(f), Dictionary::new());

    // 6: a CID font whose CFF program sits in an OpenType file.
    let cff = minimal_cff("DocuraCidOtf", &[('A', 600), ('B', 700)]);
    let otf = opentype_with_cff(&cff, &[500, 600, 700]);
    let ff = b.doc.add_object(Stream::new(dictionary! { "Subtype" => "OpenType" }, otf));
    let fd = descriptor(&mut b, "DocuraCidOtf", "FontFile3", ff, 4);
    let cf = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType0", "BaseFont" => "DocuraCidOtf",
        "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
        "FontDescriptor" => fd, "DW" => 500, "W" => vec![1.into(), Object::Array(vec![600.into(), 700.into()])],
    });
    let tu = to_unicode(&mut b, &[(1, "A"), (2, "B")], 2);
    let f = b.doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "DocuraCidOtf", "Encoding" => "Identity-H", "DescendantFonts" => vec![cf.into()], "ToUnicode" => tu });
    b.page(b"BT /F1 24 Tf 72 700 Td <000200010002> Tj ET", font_res(f), Dictionary::new());
    b.finish()
}

/// Annotations and form fields in their less common shapes, actions with
/// follow-ups, and optional content that hides annotations and XObjects.
pub fn edge_annotations() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    let page_id = b.doc.new_object_id();
    let mut annots: Vec<Object> = Vec::new();

    // Check box: the field type sits on the parent, /N is one stream.
    let parent = b.doc.new_object_id();
    let on = form(&mut b, 15, 15, b"0 0 15 15 re f", Dictionary::new());
    let kid = b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Widget", "Parent" => parent, "Rect" => rect(72, 700, 87, 715), "P" => page_id, "F" => 4, "AP" => dictionary! { "N" => on } });
    b.doc.objects.insert(parent, Object::Dictionary(dictionary! { "FT" => "Btn", "T" => Object::string_literal("haken"), "V" => "On", "Kids" => vec![kid.into()] }));
    annots.push(kid.into());
    // Check box without any appearance, state named by /AS.
    let cb = b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "T" => Object::string_literal("ohne"), "V" => "Ja", "AS" => "Ja", "Rect" => rect(100, 700, 115, 715), "P" => page_id, "F" => 4 });
    annots.push(cb.into());
    // Text field: value and default appearance on the parent, no appearance.
    let tparent = b.doc.new_object_id();
    let tkid = b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Widget", "Parent" => tparent, "Rect" => rect(150, 700, 350, 720), "P" => page_id, "F" => 4 });
    b.doc.objects.insert(tparent, Object::Dictionary(dictionary! { "FT" => "Tx", "T" => Object::string_literal("geerbt"), "V" => Object::string_literal("Wert vom Elternfeld"), "DA" => Object::string_literal("/Helv 11 Tf 0.2 0.2 0.2 rg"), "Q" => 2, "Kids" => vec![tkid.into()] }));
    annots.push(tkid.into());
    // List box with two selected values and no appearance.
    let list = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Ch", "T" => Object::string_literal("liste"), "Ff" => 1 << 21,
        "Opt" => vec![Object::string_literal("Eins"), Object::string_literal("Zwei"), Object::string_literal("Drei")],
        "V" => vec![Object::string_literal("Eins"), Object::string_literal("Drei")],
        "DA" => Object::string_literal("/Helv 10 Tf 0 g"), "Rect" => rect(380, 680, 520, 720), "P" => page_id, "F" => 4,
    });
    annots.push(list.into());

    // Stamps whose /N is a dictionary of states (with and without /AS).
    let s1 = form(&mut b, 100, 40, b"1 0 0 rg 0 0 100 40 re f", Dictionary::new());
    let s2 = form(&mut b, 100, 40, b"0 0 1 rg 0 0 100 40 re f", Dictionary::new());
    annots.push(b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Stamp", "Rect" => rect(72, 600, 172, 640), "F" => 4, "AS" => "Blau", "AP" => dictionary! { "N" => dictionary! { "Rot" => s1, "Blau" => s2 } } }).into());
    annots.push(b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Square", "Rect" => rect(200, 600, 300, 640), "F" => 4, "AP" => dictionary! { "N" => dictionary! { "Rot" => s1 } } }).into());

    // Types PDF/A does not know: painted into the page where they have an
    // appearance (state dictionary, matrix), dropped otherwise.
    let m1 = form(&mut b, 50, 50, b"0 1 0 rg 0 0 50 50 re f", Dictionary::new());
    annots.push(b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Movie", "Rect" => rect(72, 500, 122, 550), "F" => 4, "AS" => "An", "AP" => dictionary! { "N" => dictionary! { "An" => m1 } } }).into());
    let m2 = form(&mut b, 50, 50, b"1 1 0 rg 0 0 50 50 re f", dictionary! { "Matrix" => vec![0.into(), 1.into(), (-1).into(), 0.into(), 50.into(), 0.into()] });
    annots.push(b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Screen", "Rect" => rect(150, 500, 200, 550), "F" => 4, "AP" => dictionary! { "N" => m2 } }).into());
    annots.push(b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Sound", "Rect" => rect(220, 500, 240, 520), "F" => 4, "AS" => "Fehlt", "AP" => dictionary! { "N" => dictionary! { "An" => m1 } } }).into());

    // Link: allowed action followed by a list of further actions; an
    // additional-actions dictionary with allowed and forbidden triggers.
    let js = dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("app.beep()") };
    let uri = |u: &str| dictionary! { "S" => "URI", "URI" => Object::string_literal(u) };
    let mut first = uri("https://example.com/1");
    first.set("Next", vec![Object::Dictionary(js.clone()), Object::Dictionary(uri("https://example.com/2"))]);
    let aa = b.doc.add_object(dictionary! { "E" => uri("https://example.com/enter"), "PO" => js.clone(), "X" => js.clone() });
    annots.push(b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Link", "Rect" => rect(72, 420, 200, 440), "F" => 4, "A" => first, "AA" => aa, "Border" => vec![0.into(), 0.into(), 0.into()] }).into());
    let mut second = uri("https://example.com/3");
    second.set("Next", js.clone());
    annots.push(b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Link", "Rect" => rect(220, 420, 340, 440), "F" => 4, "A" => second, "AA" => dictionary! { "D" => js } }).into());

    // Optional content: a hidden group hides an annotation directly and an
    // XObject through a membership dictionary.
    let shown = b.doc.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Sichtbar") });
    let hidden = b.doc.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Versteckt") });
    let ocmd = b.doc.add_object(dictionary! { "Type" => "OCMD", "OCGs" => vec![hidden.into()] });
    let ocmd_one = b.doc.add_object(dictionary! { "Type" => "OCMD", "OCGs" => hidden });
    let h1 = form(&mut b, 80, 80, b"1 0 0 rg 0 0 80 80 re f", dictionary! { "OC" => ocmd });
    let h2 = form(&mut b, 80, 80, b"0 0 1 rg 0 0 80 80 re f", dictionary! { "OC" => ocmd_one });
    let ap = form(&mut b, 60, 30, b"0 0 60 30 re f", Dictionary::new());
    annots.push(b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Square", "Rect" => rect(72, 300, 132, 330), "F" => 4, "OC" => hidden, "AP" => dictionary! { "N" => ap } }).into());
    annots.push(b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Square", "Rect" => rect(150, 300, 210, 330), "F" => 4, "OC" => shown, "AP" => dictionary! { "N" => ap } }).into());

    let mut c = TEXT.to_vec();
    c.extend_from_slice(b"q 1 0 0 1 300 200 cm /H1 Do Q q 1 0 0 1 400 200 cm /H2 Do Q\n");
    let cid = b.doc.add_object(Stream::new(Dictionary::new(), c));
    b.doc.objects.insert(page_id, Object::Dictionary(dictionary! {
        "Type" => "Page", "Parent" => b.pages, "MediaBox" => rect(0, 0, 595, 842), "Contents" => cid,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => f }, "XObject" => dictionary! { "H1" => h1, "H2" => h2 } },
        "Annots" => annots,
    }));
    b.kids.push(page_id.into());

    // Second page: an annotation with constant opacity (PDF/A-1 renders it).
    let ap2 = form(&mut b, 100, 50, b"1 0.5 0 rg 0 0 100 50 re f", Dictionary::new());
    let semi = b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Square", "Rect" => rect(72, 600, 172, 650), "F" => 4, "CA" => 0.5, "AP" => dictionary! { "N" => ap2 } });
    b.page(TEXT, font_res(f), dictionary! { "Annots" => vec![semi.into()], "Rotate" => 90 });

    let acro = dictionary! { "Fields" => vec![parent.into(), cb.into(), tparent.into(), list.into()], "DA" => Object::string_literal("/Helv 0 Tf 0 g"), "DR" => dictionary! { "Font" => dictionary! { "Helv" => f } } };
    let ocp = dictionary! { "OCGs" => vec![shown.into(), hidden.into()], "D" => dictionary! { "Name" => Object::string_literal("Standard"), "Order" => vec![shown.into(), hidden.into()], "OFF" => vec![hidden.into()] } };
    b.finish_with(dictionary! { "AcroForm" => acro, "OCProperties" => ocp })
}

/// Colour and stream shapes: named DeviceCMYK, inline images, a tiling
/// pattern, CMYK transparency groups, a broken ICC profile, filter arrays,
/// resources inherited from the page tree.
pub fn edge_color() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);

    // 1: CMYK by name, inline images, tiling pattern, broken ICC profile.
    let pattern = b.doc.add_object(Stream::new(
        dictionary! { "Type" => "Pattern", "PatternType" => 1, "PaintType" => 1, "TilingType" => 1, "BBox" => rect(0, 0, 10, 10), "XStep" => 10, "YStep" => 10, "Resources" => Dictionary::new() },
        b"0 1 0 0 k 0 0 5 5 re f".to_vec(),
    ));
    let bad_icc = b.doc.add_object(Stream::new(dictionary! { "N" => 3 }, b"this is not an ICC profile".to_vec()));
    let res = dictionary! {
        "Font" => dictionary! { "F1" => f },
        "Pattern" => dictionary! { "P1" => pattern },
        "ColorSpace" => dictionary! { "Bad" => vec![name("ICCBased"), bad_icc.into()] },
    };
    let mut c = TEXT.to_vec();
    c.extend_from_slice(b"/DeviceCMYK cs 0 0.6 0.9 0 sc 72 600 80 80 re f /DeviceCMYK CS 0.9 0 0 0 SC 4 w 172 600 80 80 re S\n");
    c.extend_from_slice(b"/Pattern cs /P1 scn 272 600 80 80 re f /Bad cs 0.2 0.4 0.8 sc 372 600 80 80 re f\n");
    c.extend_from_slice(b"q 40 0 0 40 72 500 cm BI /W 1 /H 1 /BPC 8 /CS /CMYK ID \x00\xff\xff\x00 EI Q\n");
    c.extend_from_slice(b"q 80 0 0 40 150 500 cm BI /W 2 /H 1 /BPC 8 /CS [/I /RGB 1 <FF000000FF00>] ID \x00\x01 EI Q\n");
    c.extend_from_slice(b"q 40 0 0 40 260 500 cm BI /W 1 /H 1 /BPC 8 /CS /G /L 1 ID \x80 EI Q\n");
    b.page(&c, res, Dictionary::new());

    // 2: transparency groups blending in DeviceCMYK (direct and indirect),
    // a blend mode array, a JPEG 2000 image with a filter array.
    let group = b.doc.add_object(dictionary! { "Type" => "Group", "S" => "Transparency", "CS" => "DeviceCMYK" });
    let fm = b.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => rect(0, 0, 100, 100), "Group" => dictionary! { "S" => "Transparency", "CS" => "DeviceCMYK", "I" => true }, "StructParents" => 7 },
        b"0 0 0 0.5 k 0 0 100 100 re f".to_vec(),
    ));
    let gs = b.doc.add_object(dictionary! { "Type" => "ExtGState", "BM" => vec![name("Multiply"), name("Normal")] });
    let jp2 = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.jp2")).unwrap();
    let mut img = Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 16, "Height" => 16, "BitsPerComponent" => 8, "Filter" => vec![name("JPXDecode")] }, jp2);
    img.allows_compression = false;
    let img = b.doc.add_object(img);
    let res = dictionary! { "Font" => dictionary! { "F1" => f }, "XObject" => dictionary! { "Fm" => fm, "Im" => img }, "ExtGState" => dictionary! { "G" => gs } };
    let mut c = TEXT.to_vec();
    c.extend_from_slice(b"q 1 0 0 1 72 500 cm /Fm Do Q q /G gs 0 0 1 rg 200 500 100 100 re f Q q 100 0 0 100 330 500 cm /Im Do Q\n");
    b.page(&c, res, dictionary! { "Group" => group });

    // 3: content split over an array object, with filter arrays and a
    // Crypt filter; resources come from the page tree.
    let part1 = {
        let z = {
            use std::io::Write;
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(b"BT /F1 14 Tf 72 760 Td (Inhalt in zwei Teilen) Tj ET\n").unwrap();
            e.finish().unwrap()
        };
        let mut hex: Vec<u8> = z.iter().flat_map(|x| format!("{x:02X}").into_bytes()).collect();
        hex.push(b'>');
        let mut s = Stream::new(dictionary! { "Filter" => vec![name("ASCIIHexDecode"), name("FlateDecode")] }, hex);
        s.allows_compression = false;
        b.doc.add_object(s)
    };
    let part2 = {
        let mut s = Stream::new(
            dictionary! { "Filter" => vec![name("Crypt")], "DecodeParms" => vec![Object::Dictionary(dictionary! { "Type" => "CryptFilterDecodeParms", "Name" => "Identity" })] },
            b"0 0 0 1 k 72 600 100 20 re f\n".to_vec(),
        );
        s.allows_compression = false;
        b.doc.add_object(s)
    };
    let contents = b.doc.add_object(Object::Array(vec![part1.into(), part2.into()]));
    let page = b.doc.add_object(dictionary! { "Type" => "Page", "Parent" => b.pages, "Contents" => contents });
    b.kids.push(page.into());
    b.pages_extra = dictionary! { "Resources" => dictionary! { "Font" => dictionary! { "F1" => f } }, "MediaBox" => rect(0, 0, 595, 842) };
    b.finish()
}

/// Attachments with incomplete file specifications and /AF links.
pub fn edge_attachments() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    // No file name at all, stream only under /UF.
    let anon = b.doc.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile" }, b"ohne Namen".to_vec()));
    let anon_fs = b.doc.add_object(dictionary! { "Type" => "Filespec", "EF" => dictionary! { "UF" => anon } });
    // Linked from the catalog's /AF only, name under /F.
    let csv = b.doc.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile", "Subtype" => name("not a mime type") }, b"a;b\n1;2\n".to_vec()));
    let csv_fs = b.doc.add_object(dictionary! { "F" => Object::string_literal("werte.csv"), "EF" => dictionary! { "F" => csv }, "AFRelationship" => "Data" });
    // File attachment annotation whose specification is not in the tree.
    let note = b.doc.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile" }, b"Notiz".to_vec()));
    let note_fs = b.doc.add_object(dictionary! { "Type" => "Filespec", "F" => Object::string_literal("werte.csv"), "EF" => dictionary! { "F" => note } });
    let annot = b.doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "FileAttachment", "Rect" => rect(500, 700, 520, 720), "FS" => note_fs, "F" => 4 });
    let names = b.doc.add_object(dictionary! { "Names" => vec![Object::string_literal("anonym"), anon_fs.into()] });
    let page = b.page(TEXT, font_res(f), dictionary! { "Annots" => vec![annot.into()] });
    if let Ok(Object::Dictionary(d)) = b.doc.get_object_mut(page) {
        d.set("AF", vec![Object::Reference(csv_fs), Object::Reference(note_fs)]);
    }
    b.finish_with(dictionary! { "Names" => dictionary! { "EmbeddedFiles" => names }, "AF" => vec![Object::Reference(csv_fs), Object::Reference(anon_fs)] })
}

/// More than a hundred objects of each kind, so the output needs more
/// than one cross-reference section's worth of entries.
pub fn many_pages() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    for i in 0..130 {
        b.page(format!("BT /F1 12 Tf 72 760 Td (Seite {}) Tj ET", i + 1).as_bytes(), font_res(f), Dictionary::new());
    }
    b.finish()
}
