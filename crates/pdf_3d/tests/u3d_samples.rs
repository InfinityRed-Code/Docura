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
    assert_eq!(render::prepare(&s).triangle_count(), 12);
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
        assert!(render::prepare(&s).triangle_count() > 0, "{f}");
    }
}

/// The mesh with faces of a single-mesh scene.
fn only_mesh(s: &u3d::Scene) -> &u3d::Mesh {
    let meshes: Vec<&u3d::Mesh> = s.meshes.iter().filter(|m| !m.faces.is_empty()).collect();
    assert_eq!(meshes.len(), 1);
    meshes[0]
}

/// Triangles as index triples, rotated so the smallest index comes first
/// (keeps the winding), sorted.
fn triangle_set(faces: impl Iterator<Item = [usize; 3]>) -> Vec<[usize; 3]> {
    let mut tris: Vec<[usize; 3]> = faces
        .map(|mut t| {
            let k = (0..3).min_by_key(|&k| t[k]).unwrap();
            t.rotate_left(k);
            t
        })
        .collect();
    tris.sort();
    tris
}

/// Intel ships several scenes twice: as a progressive mesh (the default of
/// its exporter) and as a plain base mesh. Both must decode to the same
/// geometry; this is the oracle for the progressive decoder.
#[test]
fn progressive_meshes_match_their_base_mesh_twins() {
    for (prog, base) in [
        ("box.u3d", "box_base.u3d"),
        ("surface.u3d", "surface_base.u3d"),
        ("plane64x64.u3d", "plane64x64_base.u3d"),
    ] {
        let p = u3d::parse(&fixture(prog)).unwrap_or_else(|e| panic!("{prog}: {e}"));
        let b = u3d::parse(&fixture(base)).unwrap_or_else(|e| panic!("{base}: {e}"));
        println!("{prog}: {}\n{base}: {}", summary(&p), summary(&b));
        assert!(p.warnings.is_empty(), "{prog}: {:?}", p.warnings);
        let (pm, bm) = (only_mesh(&p), only_mesh(&b));
        assert_eq!(pm.positions.len(), bm.positions.len(), "{prog}");
        assert_eq!(pm.faces.len(), bm.faces.len(), "{prog}");

        // Progressive positions are quantized: map each one to the nearest
        // base position; the mapping must be one-to-one and close.
        let extent = bm
            .positions
            .iter()
            .flat_map(|p| p.iter())
            .fold(0f32, |a, v| a.max(v.abs()));
        let tolerance = extent * 1e-3;
        let map: Vec<usize> = pm
            .positions
            .iter()
            .map(|q| {
                let (i, d) = bm
                    .positions
                    .iter()
                    .map(|b| (0..3).map(|k| (b[k] - q[k]).powi(2)).sum::<f32>().sqrt())
                    .enumerate()
                    .fold(
                        (0, f32::MAX),
                        |acc, (i, d)| if d < acc.1 { (i, d) } else { acc },
                    );
                assert!(
                    d <= tolerance,
                    "{prog}: position {q:?} is {d} from the base mesh"
                );
                i
            })
            .collect();
        let mut seen = map.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), map.len(), "{prog}: positions collapse");
        let pt = triangle_set(pm.faces.iter().map(|f| f.position.map(|i| map[i as usize])));
        let bt = triangle_set(bm.faces.iter().map(|f| f.position.map(|i| i as usize)));
        assert_eq!(pt, bt, "{prog}: triangles differ");

        // Decoded normals are unit length (prediction + rotation).
        for n in &pm.normals {
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((l - 1.0).abs() < 0.05, "{prog}: normal length {l}");
        }
        assert!(
            pm.faces.iter().all(|f| f.normal.is_some()),
            "{prog}: missing normals"
        );
    }
}

#[test]
fn progressive_mesh_with_vertex_colors() {
    let s = u3d::parse(&fixture("VertexColor.u3d")).expect("parse");
    println!("{}", summary(&s));
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    let teapot = s
        .meshes
        .iter()
        .find(|m| m.name == "teapot")
        .expect("teapot");
    assert_eq!(teapot.faces.len() as u32, teapot.max_faces);
    assert!(!teapot.diffuse.is_empty());
    assert!(teapot.faces.iter().all(|f| f.diffuse.is_some()));
    let r = render::prepare(&s);
    assert!(r.geometries.iter().any(|g| !g.colors.is_empty()));
    assert!(r.materials.iter().any(|m| m.vertex_colors));
}

#[test]
fn progressive_files_with_textures_parse() {
    for f in ["TextureLayers.u3d", "plane64x64.u3d"] {
        let s = u3d::parse(&fixture(f)).unwrap_or_else(|e| panic!("{f}: {e}"));
        println!("{f}: {}", summary(&s));
        assert!(!s.textures.is_empty(), "{f}");
        for t in &s.textures {
            let magic_ok =
                t.data.starts_with(&[0x89, b'P', b'N', b'G']) || t.data.starts_with(&[0xFF, 0xD8]);
            assert!(magic_ok, "{f}: texture {} is not PNG/JPEG", t.name);
        }
    }
}

#[test]
fn damaged_files_fail_soft() {
    let good = fixture("surface.u3d");
    // Truncations at many points: never panic, never loop.
    for cut in (64..good.len()).step_by(97) {
        let _ = u3d::parse(&good[..cut]);
    }
    // Flipped bytes inside the mesh data.
    for i in (200..good.len()).step_by(131) {
        let mut bad = good.clone();
        bad[i] ^= 0x5A;
        let _ = u3d::parse(&bad);
    }
    assert!(matches!(
        u3d::parse(b"not a u3d file at all"),
        Err(u3d::U3dError::NotU3d)
    ));
}

/// Texture layers: every layer of a shader reaches the renderer with its
/// blend function, and a channel without texture coordinates is left out.
#[test]
fn texture_layers_and_blend_functions() {
    // TextureLayers.u3d: shader 1 uses channels 0 ("Texture2") and 2
    // ("Texture RGB"), the mesh carries two coordinate layers (0 and 1);
    // shader 2 uses channel 0 ("Lines Texture"). All layers are stored
    // with blend function 2 = multiply (the Intel reference's default).
    let s = u3d::parse(&fixture("TextureLayers.u3d")).unwrap();
    let sh = s.shaders.iter().find(|x| x.name == "ModelShader1").unwrap();
    assert_eq!(sh.layers.iter().map(|l| (l.channel, l.texture.as_str(), l.blend_function)).collect::<Vec<_>>(), vec![(0, "Texture2", 2), (2, "Texture RGB", 2)]);
    assert!(sh.layers.iter().all(|l| l.mode == 0 && l.blend_source == 1 && l.blend_constant == 0.5 && l.repeat == 3 && l.transform == u3d::IDENTITY));
    let mesh = only_mesh(&s);
    assert_eq!(mesh.shading.iter().map(|d| d.texture_layers).collect::<Vec<_>>(), vec![2, 1]);
    for f in &mesh.faces {
        assert_eq!(f.tex.len(), mesh.shading[f.shading as usize].texture_layers as usize);
    }

    let r = render::prepare(&s);
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    let name = |i: u32| r.textures[i as usize].name.as_str();
    let mut seen = Vec::new();
    for inst in &r.instances {
        let (g, m) = (&r.geometries[inst.geometry as usize], &r.materials[inst.material as usize]);
        for set in &g.uvs {
            assert_eq!(set.len() / 2, g.positions.len() / 3, "one uv per vertex");
        }
        let layers: Vec<_> = m.layers.iter().map(|l| (name(l.texture), l.uv, l.blend)).collect();
        seen.push((g.uvs.len(), layers));
    }
    seen.sort();
    assert_eq!(seen, vec![(1, vec![("Lines Texture", 0, "multiply")]), (2, vec![("Texture2", 0, "multiply")])]);
}

/// Blend functions as stored (values of the Intel reference), and layers
/// the viewer cannot draw are named in the warnings.
#[test]
fn blend_function_values_and_unsupported_layers() {
    let mut s = u3d::parse(&fixture("TextureLayers.u3d")).unwrap();
    let texture = |s: &u3d::Scene, i: usize| s.shaders.iter().find(|x| x.name == "ModelShader1").unwrap().layers[i].clone();
    let base = texture(&s, 0);
    let layers = |s: &mut u3d::Scene, v: Vec<u3d::TextureLayer>| s.shaders.iter_mut().find(|x| x.name == "ModelShader1").unwrap().layers = v;

    // Channels 0 and 1 both have coordinates in this mesh.
    let with = |f: u8, channel: u32| u3d::TextureLayer { blend_function: f, channel, ..base.clone() };
    layers(&mut s, vec![with(0, 0), with(3, 1)]);
    let r = render::prepare(&s);
    let m = r.materials.iter().find(|m| m.layers.len() == 2).expect("two layers drawn");
    assert_eq!(m.layers.iter().map(|l| (l.uv, l.blend, l.blend_alpha)).collect::<Vec<_>>(), vec![(0, "replace", false), (1, "blend", false)]);
    layers(&mut s, vec![with(1, 0), u3d::TextureLayer { blend_source: 0, ..with(3, 1) }]);
    let r = render::prepare(&s);
    let m = r.materials.iter().find(|m| m.layers.len() == 2).unwrap();
    assert_eq!(m.layers.iter().map(|l| (l.blend, l.blend_alpha)).collect::<Vec<_>>(), vec![("add", false), ("blend", true)]);

    // Generated coordinates, an unknown function, an intensity.
    layers(&mut s, vec![u3d::TextureLayer { mode: 3, ..with(2, 0) }, u3d::TextureLayer { intensity: 0.5, ..with(9, 1) }]);
    let r = render::prepare(&s);
    let m = r.materials.iter().find(|m| m.layers.iter().any(|l| l.uv == 1)).unwrap();
    assert_eq!(m.layers.len(), 1, "the spherical layer is not drawn");
    assert_eq!(m.layers[0].blend, "multiply");
    let w = r.warnings.join("\n");
    assert!(w.contains("texture layer 1 of shader ModelShader1: spherical mapping not shown"), "{w}");
    assert!(w.contains("texture layer 2 of shader ModelShader1: blend function 9 unknown, multiplied"), "{w}");
    assert!(w.contains("texture layer 2 of shader ModelShader1: intensity 0.5 not applied"), "{w}");
}
