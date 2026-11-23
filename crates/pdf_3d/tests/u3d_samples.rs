//! Decodes the Intel U3D test scenes (see tests/fixtures/intel/README.md).

use pdf_3d::{render, u3d};

fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/intel")
        .join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn summary(s: &u3d::Scene) -> String {
    let meshes: Vec<String> = s
        .meshes
        .iter()
        .map(|m| {
            format!(
                "{}: {}/{} pos, {}/{} faces",
                m.name,
                m.positions.len(),
                m.max_positions,
                m.faces.len(),
                m.max_faces
            )
        })
        .collect();
    format!(
        "nodes={} meshes=[{}] warnings={:?}",
        s.nodes.len(),
        meshes.join("; "),
        s.warnings
    )
}

#[test]
fn box_base_mesh_decodes_fully() {
    let s = u3d::parse(&fixture("box_base.u3d")).expect("parse");
    println!("{}", summary(&s));
    let m = s
        .meshes
        .iter()
        .find(|m| !m.faces.is_empty())
        .expect("mesh with faces");
    assert_eq!(m.positions.len() as u32, m.max_positions);
    assert_eq!(m.faces.len() as u32, m.max_faces);
    assert_eq!(m.faces.len(), 12, "a box has 12 triangles");
    // Every position is a box corner: 8 distinct points.
    let mut corners: Vec<[i32; 3]> = m
        .positions
        .iter()
        .map(|p| {
            [
                p[0].round() as i32,
                p[1].round() as i32,
                p[2].round() as i32,
            ]
        })
        .collect();
    corners.sort();
    corners.dedup();
    assert_eq!(corners.len(), 8, "{corners:?}");
    let batches = render::flatten(&s);
    let tris: usize = batches.iter().map(|b| b.positions.len() / 9).sum();
    assert_eq!(tris, 12);
}

#[test]
fn other_base_meshes_decode_fully() {
    for f in [
        "surface_base.u3d",
        "shading2_base.u3d",
        "plane64x64_base.u3d",
    ] {
        let s = u3d::parse(&fixture(f)).unwrap_or_else(|e| panic!("{f}: {e}"));
        println!("{f}: {}", summary(&s));
        assert!(!s.meshes.is_empty(), "{f}");
        for m in &s.meshes {
            assert_eq!(m.faces.len() as u32, m.max_faces, "{f} {}", m.name);
            assert_eq!(m.positions.len() as u32, m.max_positions, "{f} {}", m.name);
        }
        assert!(!render::flatten(&s).is_empty(), "{f}");
    }
}

#[test]
fn progressive_files_parse_without_error() {
    for f in ["box.u3d", "TextureLayers.u3d"] {
        let s = u3d::parse(&fixture(f)).unwrap_or_else(|e| panic!("{f}: {e}"));
        println!("{f}: {}", summary(&s));
        assert!(!s.nodes.is_empty());
    }
}
