//! U3D writer for triangle meshes.
//!
//! Writes the "no compression" profile (ECMA-363 9.4.1.2): every value is
//! plain little-endian, and each mesh is a single CLOD base mesh at full
//! resolution (no progressive refinement). Readers must support this mode;
//! Acrobat does. Block order follows what Acrobat accepts: a shader is
//! written before the material it references.

use super::*;

/// One mesh to export. Positions and the optional per-position normals are
/// in model units; `transform` places the mesh in the world.
#[derive(Debug, Clone, Default)]
pub struct ExportMesh {
    pub name: String,
    /// Name of an [`ExportGroup`] this mesh hangs under; `None` = world.
    /// `transform` is relative to that group.
    pub parent: Option<String>,
    pub positions: Vec<[f32; 3]>,
    /// Empty, or one normal per position.
    pub normals: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    /// RGBA, components 0..1.
    pub color: [f32; 4],
    pub transform: Mat4,
}

/// A group node (assembly, storey, …) shown as a folder in the model tree.
#[derive(Debug, Clone, Default)]
pub struct ExportGroup {
    /// Must be unique among groups.
    pub name: String,
    /// Parent group, declared before this one; `None` = world.
    pub parent: Option<String>,
    pub transform: Mat4,
}

#[derive(Debug, Clone, Default)]
pub struct ExportScene {
    /// Group hierarchy, parents before children.
    pub groups: Vec<ExportGroup>,
    pub meshes: Vec<ExportMesh>,
    /// Metres per unit (0.001 = millimetres); `None` = unitless.
    pub units_m: Option<f64>,
}

#[derive(Default)]
struct W {
    b: Vec<u8>,
}

impl W {
    fn u32(&mut self, v: u32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn string(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let n = bytes.len().min(u16::MAX as usize);
        self.b.extend_from_slice(&(n as u16).to_le_bytes());
        self.b.extend_from_slice(&bytes[..n]);
    }
    fn pad4(&mut self) {
        while !self.b.len().is_multiple_of(4) {
            self.b.push(0);
        }
    }
    /// Appends a complete block (type, sizes, data, padding).
    fn block(&mut self, block_type: u32, data: W) {
        self.u32(block_type);
        self.u32(data.b.len() as u32);
        self.u32(0);
        self.b.extend_from_slice(&data.b);
        self.pad4();
    }
}

fn data(f: impl FnOnce(&mut W)) -> W {
    let mut w = W::default();
    f(&mut w);
    w
}

fn parent(w: &mut W, parent: &str, transform: &Mat4) {
    w.u32(1);
    w.string(parent);
    for v in transform {
        w.f32(*v);
    }
}

/// Modifier chain wrapping the given modifier blocks.
fn chain(name: &str, chain_type: u32, modifiers: Vec<(u32, W)>) -> W {
    data(|w| {
        w.string(name);
        w.u32(chain_type);
        w.u32(0); // no bounding information
        w.pad4();
        w.u32(modifiers.len() as u32);
        for (t, d) in modifiers {
            w.block(t, d);
        }
    })
}

/// Unique, non-empty U3D names (names key the palettes; group names are
/// taken first).
fn unique_names(meshes: &[ExportMesh], groups: &[ExportGroup]) -> Vec<String> {
    let mut used: std::collections::HashSet<String> = groups.iter().map(|g| g.name.clone()).collect();
    meshes
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let base: String = m.name.trim().chars().take(200).collect();
            let base = if base.is_empty() {
                format!("Mesh{}", i + 1)
            } else {
                base
            };
            let mut name = base.clone();
            let mut k = 1;
            while !used.insert(name.clone()) {
                k += 1;
                name = format!("{base}_{k}");
            }
            name
        })
        .collect()
}

/// Encodes `scene` as a U3D file.
pub fn write(scene: &ExportScene) -> Vec<u8> {
    let names = unique_names(&scene.meshes, &scene.groups);
    let groups: std::collections::HashSet<&str> = scene.groups.iter().map(|g| g.name.as_str()).collect();
    let parent_of = |p: &Option<String>| -> String {
        p.as_deref().filter(|n| groups.contains(n)).unwrap_or("").to_string()
    };

    // One shader + material per distinct color.
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let color_index: Vec<usize> = scene
        .meshes
        .iter()
        .map(|m| match colors.iter().position(|c| *c == m.color) {
            Some(i) => i,
            None => {
                colors.push(m.color);
                colors.len() - 1
            }
        })
        .collect();

    let mut decl = W::default();
    for (ci, c) in colors.iter().enumerate() {
        decl.block(
            block::LIT_TEXTURE_SHADER,
            data(|w| {
                w.string(&format!("Shader{ci}"));
                w.u32(1); // lighting enabled
                w.f32(0.0); // alpha test reference
                w.u32(0x617); // alpha test: ALWAYS
                w.u32(0x606); // blend: FB_ALPHA_BLEND
                w.u32(1); // render pass 0
                w.u32(0); // no texture channels
                w.u32(0);
                w.string(&format!("Material{ci}"));
            }),
        );
        decl.block(
            block::MATERIAL_RESOURCE,
            data(|w| {
                w.string(&format!("Material{ci}"));
                w.u32(0x3F); // all attributes
                let rgb = [c[0], c[1], c[2]];
                for v in rgb.iter().map(|v| v * 0.25) {
                    w.f32(v); // ambient
                }
                for v in rgb {
                    w.f32(v); // diffuse
                }
                for _ in 0..3 {
                    w.f32(0.25); // specular
                }
                for _ in 0..3 {
                    w.f32(0.0); // emissive
                }
                w.f32(0.1); // reflectivity
                w.f32(c[3]); // opacity
            }),
        );
    }

    for g in &scene.groups {
        let node = data(|w| {
            w.string(&g.name);
            parent(w, &parent_of(&g.parent), &g.transform);
        });
        decl.block(block::MODIFIER_CHAIN, chain(&g.name, 0, vec![(block::GROUP_NODE, node)]));
    }

    let mut cont = W::default();
    for (i, m) in scene.meshes.iter().enumerate() {
        let name = &names[i];
        let mesh_name = format!("{name}-mesh");
        let has_normals = !m.normals.is_empty() && m.normals.len() == m.positions.len();
        let positions = m.positions.len() as u32;

        let model_node = data(|w| {
            w.string(name);
            parent(w, &parent_of(&m.parent), &m.transform);
            w.string(&mesh_name);
            w.u32(3); // front and back visible
        });
        let shading = data(|w| {
            w.string(name);
            w.u32(1); // chain index
            w.u32(1); // applies to meshes
            w.u32(1); // one shader list
            w.u32(1); // one shader
            w.string(&format!("Shader{}", color_index[i]));
        });
        let node_chain = chain(
            name,
            0,
            vec![
                (block::MODEL_NODE, model_node),
                (block::SHADING_MODIFIER, shading),
            ],
        );
        decl.block(block::MODIFIER_CHAIN, node_chain);

        let declaration = data(|w| {
            w.string(&mesh_name);
            w.u32(0); // chain index
            w.u32(if has_normals { 0 } else { 1 }); // exclude normals
            w.u32(m.triangles.len() as u32);
            w.u32(positions);
            w.u32(if has_normals { positions } else { 0 });
            w.u32(0); // diffuse colors
            w.u32(0); // specular colors
            w.u32(0); // texture coords
            w.u32(1); // shading count
            w.u32(0); // shading attributes
            w.u32(0); // texture layers
            w.u32(0); // original shading id
            w.u32(positions); // minimum resolution = everything in the base mesh
            w.u32(positions); // final maximum resolution
            for _ in 0..3 {
                w.u32(1000); // quality factors (unused without progressive data)
            }
            for _ in 0..5 {
                w.f32(1.0); // inverse quantization
            }
            w.f32(0.0); // normal crease
            w.f32(0.0); // normal update
            w.f32(0.0); // normal tolerance
            w.u32(0); // bones
        });
        decl.block(
            block::MODIFIER_CHAIN,
            chain(
                &mesh_name,
                1,
                vec![(block::CLOD_MESH_DECLARATION, declaration)],
            ),
        );

        cont.block(
            block::CLOD_BASE_MESH,
            data(|w| {
                w.string(&mesh_name);
                w.u32(0);
                w.u32(m.triangles.len() as u32);
                w.u32(positions);
                w.u32(if has_normals { positions } else { 0 });
                w.u32(0);
                w.u32(0);
                w.u32(0);
                for p in &m.positions {
                    p.iter().for_each(|v| w.f32(*v));
                }
                if has_normals {
                    for n in &m.normals {
                        n.iter().for_each(|v| w.f32(*v));
                    }
                }
                for t in &m.triangles {
                    w.u32(0); // shading id
                    for &i in t {
                        w.u32(i);
                        if has_normals {
                            w.u32(i);
                        }
                    }
                }
            }),
        );
    }

    let priority = |p: u32| data(|w| w.u32(p));
    let mut header = data(|w| {
        w.u32(0); // version 0.0
        w.u32(if scene.units_m.is_some() {
            0x4 | 0x8
        } else {
            0x4
        });
        w.u32(0); // declaration size, patched below
        w.u64(0); // file size, patched below
        w.u32(106); // UTF-8
        if let Some(u) = scene.units_m {
            w.f64(u);
        }
    });
    let mut file = W::default();
    // Header size is fixed, so the declaration size is known up front.
    let header_len = 12 + header.b.len().div_ceil(4) * 4;
    let mut body = W::default();
    body.block(block::PRIORITY_UPDATE, priority(0));
    body.b.extend_from_slice(&decl.b);
    let declaration_size = header_len + body.b.len();
    body.block(block::PRIORITY_UPDATE, priority(0x100));
    body.b.extend_from_slice(&cont.b);
    let file_size = header_len + body.b.len();
    header.b[8..12].copy_from_slice(&(declaration_size as u32).to_le_bytes());
    header.b[12..20].copy_from_slice(&(file_size as u64).to_le_bytes());
    file.block(block::FILE_HEADER, header);
    file.b.extend_from_slice(&body.b);
    file.b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tetra(name: &str, color: [f32; 4]) -> ExportMesh {
        ExportMesh {
            name: name.into(),
            parent: None,
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            normals: vec![],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
            color,
            transform: IDENTITY,
        }
    }

    #[test]
    fn roundtrip_through_parser() {
        let mut moved = tetra("b", [0.2, 0.4, 0.6, 1.0]);
        moved.transform[12] = 5.0;
        let scene = ExportScene {
            groups: Vec::new(),
            meshes: vec![
                tetra("a", [1.0, 0.0, 0.0, 1.0]),
                moved,
                tetra("a", [1.0, 0.0, 0.0, 0.5]),
            ],
            units_m: Some(0.001),
        };
        let bytes = write(&scene);
        assert_eq!(&bytes[..4], b"U3D\0");
        let s = parse(&bytes).expect("parse");
        assert_eq!(s.units_m, Some(0.001));
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
        assert_eq!(s.meshes.len(), 3);
        assert_eq!(s.meshes[0].positions, scene.meshes[0].positions);
        let tris: Vec<[u32; 3]> = s.meshes[1].faces.iter().map(|f| f.position).collect();
        assert_eq!(tris, scene.meshes[1].triangles);
        let names: Vec<&str> = s.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "a_2"]);
        assert_eq!(s.nodes[1].parents[0].transform[12], 5.0);
        assert_eq!(s.nodes[2].shading, vec![vec!["Shader2".to_string()]]);
        assert_eq!(s.materials.len(), 3);
        assert_eq!(s.materials[2].opacity, 0.5);
    }

    #[test]
    fn groups_form_the_model_tree() {
        let mut lifted = IDENTITY;
        lifted[14] = 3.0;
        let mut column = tetra("Column", [0.5, 0.5, 0.5, 1.0]);
        column.parent = Some("Level 1".into());
        let mut beam = tetra("Beam", [0.5, 0.5, 0.5, 1.0]);
        beam.parent = Some("Level 2".into());
        let scene = ExportScene {
            groups: vec![
                ExportGroup { name: "Building".into(), parent: None, transform: IDENTITY },
                ExportGroup { name: "Level 1".into(), parent: Some("Building".into()), transform: IDENTITY },
                ExportGroup { name: "Level 2".into(), parent: Some("Building".into()), transform: lifted },
            ],
            meshes: vec![column, beam],
            units_m: None,
        };
        let s = parse(&write(&scene)).expect("parse");
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
        let r = crate::render::prepare(&s);
        let beam = r.instances.iter().find(|i| i.node == "Beam").expect("beam");
        assert_eq!(beam.path, ["Building", "Level 2", "Beam"]);
        assert_eq!(beam.matrix[14], 3.0, "group transform applies");
        assert_eq!(r.tree.iter().filter(|n| n.kind == "group").count(), 3);
    }
}
