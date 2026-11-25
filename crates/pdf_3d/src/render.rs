//! Turns a decoded scene into what a WebGL renderer uploads: indexed
//! geometries in model space (one per mesh and shading ID), instances that
//! place them (one per path from a model node to the world, so nodes with
//! several parents are instanced), materials, textures, the node tree for
//! a model tree UI, and the cameras and lights of the file.
//!
//! [`pack`] serializes the result into one binary buffer for IPC.

use std::collections::HashMap;

use crate::u3d::{Mat4, Mesh, NodeKind, Scene, IDENTITY};
use serde::Serialize;

#[derive(Debug, Clone, Default)]
pub struct Geometry {
    /// xyz per vertex.
    pub positions: Vec<f32>,
    /// xyz per vertex; empty when the file has no normals (the renderer
    /// computes flat ones).
    pub normals: Vec<f32>,
    /// rgb per vertex; empty without vertex colors.
    pub colors: Vec<f32>,
    /// uv per vertex; empty without texture coordinates.
    pub uvs: Vec<f32>,
    /// Three per triangle.
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Instance {
    /// Model node name.
    pub node: String,
    /// Node names from the outermost ancestor down to `node`.
    pub path: Vec<String>,
    /// Model-to-world matrix (column-major).
    pub matrix: Mat4,
    pub geometry: u32,
    pub material: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RenderMaterial {
    /// RGBA (alpha = material opacity).
    pub diffuse: [f32; 4],
    pub ambient: [f32; 3],
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    /// Index into `RenderScene::textures`.
    pub texture: Option<u32>,
    pub vertex_colors: bool,
    pub lighting: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeNode {
    pub name: String,
    /// "group", "model", "light" or "view".
    pub kind: &'static str,
    /// Parent node names ("" = world).
    pub parents: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Camera {
    pub name: String,
    /// Camera-to-world matrix; the camera looks along -z with +y up.
    pub matrix: Mat4,
    pub orthographic: bool,
    /// Field of view in degrees, or orthographic height.
    pub projection: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Light {
    /// 0 ambient, 1 directional, 2 point, 3 spot.
    pub light_type: u8,
    pub color: [f32; 3],
    pub intensity: f32,
    pub spot_angle: f32,
    /// Light-to-world matrix; directional and spot lights shine along -z.
    pub matrix: Mat4,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextureRef {
    pub name: String,
    pub mime: String,
    #[serde(skip)]
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct RenderScene {
    pub geometries: Vec<Geometry>,
    pub instances: Vec<Instance>,
    pub materials: Vec<RenderMaterial>,
    pub textures: Vec<TextureRef>,
    pub tree: Vec<TreeNode>,
    pub cameras: Vec<Camera>,
    pub lights: Vec<Light>,
    /// World-space bounds of all instances.
    pub bounds: Option<([f32; 3], [f32; 3])>,
    pub units_m: Option<f64>,
    pub warnings: Vec<String>,
}

impl RenderScene {
    pub fn triangle_count(&self) -> usize {
        self.instances
            .iter()
            .map(|i| self.geometries[i.geometry as usize].indices.len() / 3)
            .sum()
    }
}

pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut r = [0f32; 16];
    for c in 0..4 {
        for row in 0..4 {
            r[c * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[c * 4 + k]).sum();
        }
    }
    r
}

fn apply(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// World transforms of node `name` with the ancestor path of each (one
/// per path to the root).
fn world_transforms(scene: &Scene, name: &str, depth: usize) -> Vec<(Mat4, Vec<String>)> {
    if depth > 64 {
        return Vec::new(); // cycle or absurd nesting
    }
    let Some(node) = scene.nodes.iter().find(|n| n.name == name) else {
        return vec![(IDENTITY, vec![name.to_string()])];
    };
    let mut out = Vec::new();
    for p in &node.parents {
        if p.name.is_empty() {
            out.push((p.transform, vec![name.to_string()]));
        } else {
            for (w, mut path) in world_transforms(scene, &p.name, depth + 1) {
                path.push(name.to_string());
                out.push((mul(&w, &p.transform), path));
            }
        }
    }
    if node.parents.is_empty() {
        out.push((IDENTITY, vec![name.to_string()]));
    }
    out
}

const DEFAULT_MATERIAL: RenderMaterial = RenderMaterial {
    diffuse: [0.75, 0.75, 0.75, 1.0],
    ambient: [0.0; 3],
    specular: [0.2; 3],
    emissive: [0.0; 3],
    texture: None,
    vertex_colors: false,
    lighting: true,
};

/// Indexed geometry for the faces of `mesh` with shading ID `sid`.
fn build_geometry(mesh: &Mesh, sid: u32) -> Geometry {
    let mut g = Geometry::default();
    let mut map: HashMap<(u32, u32, u32, u32), u32> = HashMap::new();
    let faces: Vec<_> = mesh.faces.iter().filter(|f| f.shading == sid).collect();
    let has_normals = !mesh.normals.is_empty() && faces.iter().all(|f| f.normal.is_some());
    let has_colors = !mesh.diffuse.is_empty() && faces.iter().all(|f| f.diffuse.is_some());
    let has_uvs = !mesh.tex_coords.is_empty() && faces.iter().all(|f| f.tex.is_some());
    for f in faces {
        for c in 0..3 {
            let key = (
                f.position[c],
                if has_normals { f.normal.map_or(0, |n| n[c]) } else { 0 },
                if has_colors { f.diffuse.map_or(0, |d| d[c]) } else { 0 },
                if has_uvs { f.tex.map_or(0, |t| t[c]) } else { 0 },
            );
            let next = map.len() as u32;
            let v = *map.entry(key).or_insert_with(|| {
                let p = mesh.positions[key.0 as usize];
                g.positions.extend_from_slice(&p);
                if has_normals {
                    g.normals.extend_from_slice(&mesh.normals[key.1 as usize]);
                }
                if has_colors {
                    g.colors.extend_from_slice(&mesh.diffuse[key.2 as usize][..3]);
                }
                if has_uvs {
                    let t = mesh.tex_coords[key.3 as usize];
                    g.uvs.extend_from_slice(&[t[0], t[1]]);
                }
                next
            });
            g.indices.push(v);
        }
    }
    g
}

pub fn prepare(scene: &Scene) -> RenderScene {
    let mut out = RenderScene {
        units_m: scene.units_m,
        warnings: scene.warnings.clone(),
        textures: scene
            .textures
            .iter()
            .map(|t| TextureRef { name: t.name.clone(), mime: t.mime.clone(), data: t.data.clone() })
            .collect(),
        ..Default::default()
    };
    // Geometry per (mesh, shading ID), built on first use.
    let mut geometries: HashMap<(usize, u32), u32> = HashMap::new();
    let mut bounds: Option<([f32; 3], [f32; 3])> = None;

    for node in &scene.nodes {
        let kind = match &node.kind {
            NodeKind::Group => "group",
            NodeKind::Model { .. } => "model",
            NodeKind::Light { .. } => "light",
            NodeKind::View(_) => "view",
        };
        out.tree.push(TreeNode { name: node.name.clone(), kind, parents: node.parents.iter().map(|p| p.name.clone()).collect() });

        match &node.kind {
            NodeKind::View(v) => {
                if let Some((m, _)) = world_transforms(scene, &node.name, 0).into_iter().next() {
                    out.cameras.push(Camera {
                        name: node.name.clone(),
                        matrix: m,
                        orthographic: v.attributes & 6 == 2,
                        projection: v.projection,
                    });
                }
            }
            NodeKind::Light { resource } => {
                if let Some(l) = scene.lights.iter().find(|l| &l.name == resource) {
                    for (m, _) in world_transforms(scene, &node.name, 0) {
                        out.lights.push(Light {
                            light_type: l.light_type,
                            color: l.color,
                            intensity: l.intensity,
                            spot_angle: l.spot_angle,
                            matrix: m,
                        });
                    }
                }
            }
            NodeKind::Model { resource, visibility } => {
                if *visibility == 0 {
                    continue;
                }
                let Some(mi) = scene.meshes.iter().position(|m| &m.name == resource) else { continue };
                let mesh = &scene.meshes[mi];
                if mesh.faces.is_empty() {
                    continue;
                }
                let mut sids: Vec<u32> = mesh.faces.iter().map(|f| f.shading).collect();
                sids.sort_unstable();
                sids.dedup();
                for (world, path) in world_transforms(scene, &node.name, 0) {
                    for &sid in &sids {
                        let gi = *geometries.entry((mi, sid)).or_insert_with(|| {
                            out.geometries.push(build_geometry(mesh, sid));
                            out.geometries.len() as u32 - 1
                        });
                        let material = material_for(scene, &node.shading, sid, &out.geometries[gi as usize], &out.textures);
                        let mi_out = match out.materials.iter().position(|m| *m == material) {
                            Some(i) => i,
                            None => {
                                out.materials.push(material);
                                out.materials.len() - 1
                            }
                        } as u32;
                        grow_bounds(&mut bounds, &out.geometries[gi as usize], &world);
                        out.instances.push(Instance {
                            node: node.name.clone(),
                            path: path.clone(),
                            matrix: world,
                            geometry: gi,
                            material: mi_out,
                        });
                    }
                }
            }
            NodeKind::Group => {}
        }
    }
    out.bounds = bounds;
    out
}

fn material_for(
    scene: &Scene,
    shading: &[Vec<String>],
    sid: u32,
    geometry: &Geometry,
    textures: &[TextureRef],
) -> RenderMaterial {
    let shader = shading
        .get(sid as usize)
        .and_then(|l| l.first())
        .and_then(|s| scene.shaders.iter().find(|x| &x.name == s))
        // No shading modifier: U3D uses the first shader of the palette
        // only when it is the default; fall back to our default instead.
        ;
    let mut m = DEFAULT_MATERIAL;
    if let Some(sh) = shader {
        m.lighting = sh.attributes & 1 != 0;
        if let Some(mat) = scene.materials.iter().find(|x| x.name == sh.material) {
            m.diffuse = [mat.diffuse[0], mat.diffuse[1], mat.diffuse[2], mat.opacity.clamp(0.0, 1.0)];
            m.ambient = mat.ambient;
            m.specular = mat.specular;
            m.emissive = mat.emissive;
        }
        if !geometry.uvs.is_empty() {
            m.texture = sh
                .textures
                .first()
                .and_then(|t| textures.iter().position(|x| &x.name == t))
                .map(|i| i as u32);
        }
        m.vertex_colors = sh.attributes & 4 != 0 && !geometry.colors.is_empty();
    } else {
        m.vertex_colors = !geometry.colors.is_empty();
    }
    m
}

/// Grows `bounds` by every vertex of `g` placed with `m`: the exact
/// axis-aligned box in world space. (The corners of the geometry's own box
/// would give a larger one for a rotated instance; viewers fit the default
/// camera to these bounds.)
fn grow_bounds(bounds: &mut Option<([f32; 3], [f32; 3])>, g: &Geometry, m: &Mat4) {
    for p in g.positions.chunks_exact(3) {
        let w = apply(m, [p[0], p[1], p[2]]);
        if !w.iter().all(|v| v.is_finite()) {
            continue;
        }
        let b = bounds.get_or_insert((w, w));
        for k in 0..3 {
            b.0[k] = b.0[k].min(w[k]);
            b.1[k] = b.1[k].max(w[k]);
        }
    }
}

/// Byte range inside the data section of a packed scene.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Range {
    pub offset: u32,
    pub length: u32,
}

#[derive(Serialize)]
struct PackedGeometry {
    positions: Range,
    normals: Option<Range>,
    colors: Option<Range>,
    uvs: Option<Range>,
    indices: Range,
}

#[derive(Serialize)]
struct PackedTexture<'a> {
    name: &'a str,
    mime: &'a str,
    data: Range,
}

#[derive(Serialize)]
struct Header<'a> {
    geometries: Vec<PackedGeometry>,
    instances: &'a [Instance],
    materials: &'a [RenderMaterial],
    textures: Vec<PackedTexture<'a>>,
    tree: &'a [TreeNode],
    cameras: &'a [Camera],
    lights: &'a [Light],
    bounds: Option<([f32; 3], [f32; 3])>,
    units_m: Option<f64>,
    warnings: &'a [String],
    triangles: usize,
}

/// Binary layout: `"D3S1"`, u32 header length, UTF-8 JSON header, zero
/// padding to 4 bytes, then the data section. Ranges in the header are byte
/// offsets into the data section; float and index arrays are 4-byte
/// aligned little-endian `f32` / `u32`.
pub fn pack(s: &RenderScene) -> Vec<u8> {
    let mut data: Vec<u8> = Vec::new();
    let mut put = |bytes: &[u8]| -> Range {
        while data.len() % 4 != 0 {
            data.push(0);
        }
        let offset = data.len() as u32;
        data.extend_from_slice(bytes);
        Range { offset, length: bytes.len() as u32 }
    };
    fn f32s(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }
    let geometries = s
        .geometries
        .iter()
        .map(|g| PackedGeometry {
            positions: put(&f32s(&g.positions)),
            normals: (!g.normals.is_empty()).then(|| put(&f32s(&g.normals))),
            colors: (!g.colors.is_empty()).then(|| put(&f32s(&g.colors))),
            uvs: (!g.uvs.is_empty()).then(|| put(&f32s(&g.uvs))),
            indices: put(&g.indices.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()),
        })
        .collect();
    let textures = s
        .textures
        .iter()
        .map(|t| PackedTexture { name: &t.name, mime: &t.mime, data: put(&t.data) })
        .collect();
    let header = Header {
        geometries,
        instances: &s.instances,
        materials: &s.materials,
        textures,
        tree: &s.tree,
        cameras: &s.cameras,
        lights: &s.lights,
        bounds: s.bounds,
        units_m: s.units_m,
        warnings: &s.warnings,
        triangles: s.triangle_count(),
    };
    let json = serde_json::to_vec(&header).expect("serialize header");
    let mut out = Vec::with_capacity(8 + json.len() + 3 + data.len());
    out.extend_from_slice(b"D3S1");
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    while out.len() % 4 != 0 {
        out.push(b' ');
    }
    out.extend_from_slice(&data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_of_a_rotated_instance_are_tight() {
        // A thin diagonal strip: its own box is 10 x 10, turned by 45
        // degrees the strip lies along the x axis.
        let g = Geometry { positions: vec![0.0, 0.0, 0.0, 10.0, 10.0, 0.0, 5.0, 5.0, 1.0], ..Default::default() };
        let (s, c) = (-std::f32::consts::FRAC_PI_4).sin_cos();
        let m: Mat4 = [c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 100.0, 0.0, 0.0, 1.0];
        let mut bounds = None;
        grow_bounds(&mut bounds, &g, &m);
        let (lo, hi) = bounds.unwrap();
        let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
        assert!(close(lo[0], 100.0) && close(hi[0], 100.0 + 200f32.sqrt()), "{lo:?} {hi:?}");
        // The corners of the 10 x 10 box would reach y = +-7.07.
        assert!(close(lo[1], 0.0) && close(hi[1], 0.0), "{lo:?} {hi:?}");
        assert!(close(lo[2], 0.0) && close(hi[2], 1.0), "{lo:?} {hi:?}");
    }
}
