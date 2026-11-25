//! Dev utility: generate 3D PDFs for testing the 3D viewer.
//!
//!     cargo run -p pdf_mutate --example make_3d_samples -- out_dir
//!
//! - `building.pdf`: a building frame like a structural CAD export
//!   (storeys as groups, ~1000 columns/beams/slabs as parts), no poster.
//! - `terrain.pdf`: one large mesh (~500k triangles), no poster.
//! - `orientation.pdf`: coloured axis bars, activated on page open, to
//!   cross-check camera convention and colours in other viewers.
//! - `blank.pdf`: an empty page to insert models into.
//! - `prc_placeholder.pdf`: a 3D annotation declaring PRC data (not a real
//!   PRC model) with a poster, for the "format not supported" path.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_3d::u3d::{ExportGroup, ExportMesh, ExportScene, IDENTITY};
use pdf_types::{RectPt, ThreeDView};

fn page_pdf(title: &str, w: f32, h: f32) -> Vec<u8> {
    let mut d = Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let font = d.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let content = format!("BT /F1 18 Tf 40 {} Td ({}) Tj ET\n", h - 50.0, title);
    let content_id = d.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let page = d.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), w.into(), h.into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    d.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }),
    );
    let catalog = d.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    d.trailer.set("Root", catalog);
    let mut out = Vec::new();
    d.save_to(&mut out).unwrap();
    out
}

/// Axis-aligned box from `lo` to `hi` (12 triangles, flat normals).
fn cuboid(name: String, parent: &str, lo: [f32; 3], hi: [f32; 3], color: [f32; 4]) -> ExportMesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut triangles = Vec::new();
    let c = |x: usize, y: usize, z: usize| [[lo[0], hi[0]][x], [lo[1], hi[1]][y], [lo[2], hi[2]][z]];
    // (normal, four corners counter-clockwise seen from outside)
    let faces: [([f32; 3], [[usize; 3]; 4]); 6] = [
        ([-1.0, 0.0, 0.0], [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]]),
        ([1.0, 0.0, 0.0], [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]]),
        ([0.0, -1.0, 0.0], [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]]),
        ([0.0, 1.0, 0.0], [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]]),
        ([0.0, 0.0, -1.0], [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]]),
        ([0.0, 0.0, 1.0], [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]]),
    ];
    for (n, quad) in faces {
        let base = positions.len() as u32;
        for q in quad {
            positions.push(c(q[0], q[1], q[2]));
            normals.push(n);
        }
        triangles.push([base, base + 1, base + 2]);
        triangles.push([base, base + 2, base + 3]);
    }
    ExportMesh { name, parent: Some(parent.into()), positions, normals, triangles, color, transform: IDENTITY }
}

fn building() -> ExportScene {
    let concrete = [0.72, 0.72, 0.70, 1.0];
    let steel = [0.30, 0.42, 0.62, 1.0];
    let slab = [0.86, 0.84, 0.78, 1.0];
    let glass = [0.55, 0.75, 0.90, 0.35];
    let (nx, ny, storeys) = (8usize, 6usize, 5usize);
    let (dx, dy, dz) = (6000.0f32, 5000.0f32, 3500.0f32); // millimetres
    let mut groups = vec![ExportGroup { name: "Gebäude".into(), parent: None, transform: IDENTITY }];
    let mut meshes = Vec::new();
    for s in 0..storeys {
        let level = format!("Geschoss {}", s + 1);
        groups.push(ExportGroup { name: level.clone(), parent: Some("Gebäude".into()), transform: IDENTITY });
        for (part, label) in [("Stützen", "Stütze"), ("Träger", "Träger"), ("Decken", "Decke"), ("Fassade", "Glas")] {
            groups.push(ExportGroup { name: format!("{level} {part}"), parent: Some(level.clone()), transform: IDENTITY });
            let g = format!("{level} {part}");
            let z0 = s as f32 * dz;
            match label {
                "Stütze" => {
                    for i in 0..=nx {
                        for j in 0..=ny {
                            let (x, y) = (i as f32 * dx, j as f32 * dy);
                            meshes.push(cuboid(format!("{label} {}-{}-{}", s + 1, i + 1, j + 1), &g, [x - 200.0, y - 200.0, z0], [x + 200.0, y + 200.0, z0 + dz - 300.0], concrete));
                        }
                    }
                }
                "Träger" => {
                    for j in 0..=ny {
                        let y = j as f32 * dy;
                        meshes.push(cuboid(format!("{label} {}-X{}", s + 1, j + 1), &g, [0.0, y - 150.0, z0 + dz - 600.0], [nx as f32 * dx, y + 150.0, z0 + dz - 300.0], steel));
                    }
                    for i in 0..=nx {
                        let x = i as f32 * dx;
                        meshes.push(cuboid(format!("{label} {}-Y{}", s + 1, i + 1), &g, [x - 150.0, 0.0, z0 + dz - 600.0], [x + 150.0, ny as f32 * dy, z0 + dz - 300.0], steel));
                    }
                }
                "Decke" => {
                    for i in 0..nx {
                        for j in 0..ny {
                            let (x, y) = (i as f32 * dx, j as f32 * dy);
                            meshes.push(cuboid(format!("{label} {}-{}-{}", s + 1, i + 1, j + 1), &g, [x, y, z0 + dz - 300.0], [x + dx, y + dy, z0 + dz], slab));
                        }
                    }
                }
                _ => {
                    meshes.push(cuboid(format!("{label} {} Süd", s + 1), &g, [0.0, -260.0, z0], [nx as f32 * dx, -240.0, z0 + dz - 300.0], glass));
                }
            }
        }
    }
    ExportScene { groups, meshes, units_m: Some(0.001) }
}

fn terrain() -> ExportScene {
    let n = 500usize; // (n-1)^2 * 2 ≈ 498k triangles
    let size = 1000.0f32;
    let height = |x: f32, y: f32| 40.0 * (x * 0.012).sin() * (y * 0.009).cos() + 15.0 * (x * 0.05 + y * 0.03).sin();
    let mut positions = Vec::with_capacity(n * n);
    let mut normals = Vec::with_capacity(n * n);
    for j in 0..n {
        for i in 0..n {
            let (x, y) = (i as f32 / (n - 1) as f32 * size, j as f32 / (n - 1) as f32 * size);
            positions.push([x, y, height(x, y)]);
            let e = 0.5;
            let (hx, hy) = (height(x + e, y) - height(x - e, y), height(x, y + e) - height(x, y - e));
            let nv = [-hx / (2.0 * e), -hy / (2.0 * e), 1.0];
            let l = (nv[0] * nv[0] + nv[1] * nv[1] + 1.0).sqrt();
            normals.push([nv[0] / l, nv[1] / l, 1.0 / l]);
        }
    }
    let mut triangles = Vec::with_capacity((n - 1) * (n - 1) * 2);
    for j in 0..n - 1 {
        for i in 0..n - 1 {
            let a = (j * n + i) as u32;
            let (b, c, d) = (a + 1, a + n as u32, a + n as u32 + 1);
            triangles.push([a, b, d]);
            triangles.push([a, d, c]);
        }
    }
    ExportScene {
        groups: Vec::new(),
        meshes: vec![ExportMesh {
            name: "Gelände".into(),
            parent: None,
            positions,
            normals,
            triangles,
            color: [0.45, 0.62, 0.36, 1.0],
            transform: IDENTITY,
        }],
        units_m: Some(1.0),
    }
}

/// The inserter's preset views (Z up), see `standardViews` in
/// `threed-camera.ts`.
fn views(lo: [f32; 3], hi: [f32; 3]) -> Vec<ThreeDView> {
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let r = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt() / 2.0;
    let dist = r / (15f32.to_radians()).sin() * 1.02;
    [
        ("Isometrisch", [1.0, -1.0, 0.8], [0.0, 0.0, 1.0]),
        ("Vorne", [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
        ("Hinten", [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ("Links", [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ("Rechts", [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ("Oben", [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ("Unten", [0.0, 0.0, -1.0], [0.0, -1.0, 0.0]),
    ]
    .into_iter()
    .map(|(name, dir, up): (&str, [f32; 3], [f32; 3])| {
        let l = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        let pos = [c[0] + dir[0] / l * dist, c[1] + dir[1] / l * dist, c[2] + dir[2] / l * dist];
        ThreeDView {
            name: name.into(),
            internal_name: Some(name.into()),
            c2w: Some(pdf_mutate::c2w_look_at(pos, c, up)),
            center_of_orbit: Some(dist),
            fov: Some(30.0),
            fov_span: Some("Min".into()),
            background: Some([1.0, 1.0, 1.0]),
            render_mode: Some("Solid".into()),
            lighting: Some("CAD".into()),
            ..Default::default()
        }
    })
    .collect()
}

/// Coloured axis bars of different lengths plus an L-shaped marker: makes
/// a wrong camera convention (mirrored, upside down) or wrong colours
/// obvious when the file is checked in another viewer.
fn orientation() -> ExportScene {
    let groups = vec![ExportGroup { name: "Achsen".into(), parent: None, transform: IDENTITY }];
    let bar = |name: &str, hi: [f32; 3], color: [f32; 4]| cuboid(name.into(), "Achsen", [0.0, 0.0, 0.0], hi, color);
    let meshes = vec![
        bar("X rot (lang)", [300.0, 20.0, 20.0], [0.90, 0.10, 0.10, 1.0]),
        bar("Y gruen (mittel)", [20.0, 200.0, 20.0], [0.10, 0.70, 0.15, 1.0]),
        bar("Z blau (kurz)", [20.0, 20.0, 120.0], [0.10, 0.25, 0.90, 1.0]),
        cuboid("Marke gelb (+X oben)".into(), "Achsen", [260.0, 0.0, 20.0], [300.0, 20.0, 60.0], [0.95, 0.80, 0.10, 1.0]),
    ];
    ExportScene { groups, meshes, units_m: Some(0.001) }
}

fn bounds(s: &ExportScene) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for m in &s.meshes {
        for p in &m.positions {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    (lo, hi)
}

fn gray_png() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(4, 3, image::Rgb([225, 228, 232]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// Writes a one-page PDF with the scene as a 3D annotation; `poster` false
/// strips the appearance so viewers must render a preview themselves.
fn write_pdf(path: &std::path::Path, title: &str, scene: &ExportScene, poster: bool) {
    write_pdf_with(path, title, scene, poster, "XA")
}

fn write_pdf_with(path: &std::path::Path, title: &str, scene: &ExportScene, poster: bool, activation: &str) {
    let u3d = pdf_3d::u3d::write(scene);
    let (lo, hi) = bounds(scene);
    let (w, h) = (842.0, 595.0);
    let mut doc = pdf_mutate::MutableDoc::from_bytes(&page_pdf(title, w, h)).unwrap();
    doc.add_3d_annotation(
        0,
        &pdf_mutate::ThreeDAnnotation {
            rect: RectPt { x0: 40.0, y0: 80.0, x1: w - 40.0, y1: h - 40.0 },
            u3d: u3d.clone(),
            views: views(lo, hi),
            default_view: 0,
            poster: gray_png(),
            name: title.into(),
            activation: activation.into(),
        },
    )
    .unwrap();
    let mut bytes = doc.to_bytes().unwrap();
    if !poster {
        let mut d = lopdf::Document::load_mem(&bytes).unwrap();
        for o in d.objects.values_mut() {
            if let Ok(dict) = o.as_dict_mut() {
                if dict.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"3D".as_slice()) {
                    dict.remove(b"AP");
                }
            }
        }
        bytes.clear();
        d.save_to(&mut bytes).unwrap();
    }
    std::fs::write(path, &bytes).unwrap();
    let tris: usize = scene.meshes.iter().map(|m| m.triangles.len()).sum();
    println!("{}: {} parts, {tris} triangles, U3D {} KB, PDF {} KB", path.display(), scene.meshes.len(), u3d.len() / 1024, bytes.len() / 1024);
}

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("blank.pdf"), page_pdf("Docura 3D-Test: Modell einfuegen", 842.0, 595.0)).unwrap();
    write_pdf(&out.join("building.pdf"), "Docura 3D-Test: Tragwerk (5 Geschosse)", &building(), false);
    write_pdf(&out.join("terrain.pdf"), "Docura 3D-Test: Gelaende (500k Dreiecke)", &terrain(), false);
    // For cross-checks in Acrobat Reader / Foxit: opens activated (/A /PO).
    // Expected in "Vorne": red X bar to the right, blue Z bar up, the yellow
    // mark on top of the right end; in "Oben": red to the right, green up.
    write_pdf_with(
        &out.join("orientation.pdf"),
        "Docura 3D-Test: Achsen. Vorne: rot (X) nach rechts, blau (Z) nach oben, gelb rechts oben",
        &orientation(),
        false,
        "PO",
    );

    // PRC placeholder: a 3D annotation whose stream says PRC.
    let mut doc = lopdf::Document::load_mem(&page_pdf("Docura 3D-Test: PRC (nicht unterstuetzt)", 842.0, 595.0)).unwrap();
    let page_id = *doc.get_pages().get(&1).unwrap();
    let img = doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 },
        vec![200, 210, 225],
    ));
    let form = doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 400.into(), 300.into()], "Resources" => dictionary! { "XObject" => dictionary! { "I" => img } } },
        b"q 400 0 0 300 0 0 cm /I Do Q".to_vec(),
    ));
    let stream = doc.add_object(Stream::new(dictionary! { "Type" => "3D", "Subtype" => "PRC" }, b"PRC\x00placeholder, not a model".to_vec()));
    let annot = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "3D", "Rect" => vec![60.into(), 120.into(), 460.into(), 420.into()],
        "3DD" => stream, "AP" => dictionary! { "N" => form }, "Contents" => Object::string_literal("PRC-Modell"),
    });
    doc.get_dictionary_mut(page_id).unwrap().set("Annots", vec![annot.into()]);
    let p = out.join("prc_placeholder.pdf");
    doc.save(&p).unwrap();
    println!("{}", p.display());
}
