//! Flattens a decoded scene into world-space triangle batches for a WebGL
//! renderer: one batch per (model node instance, shading ID), non-indexed,
//! with per-corner normals.

use crate::u3d::{Mat4, NodeKind, Scene, IDENTITY};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Batch {
    /// Model node name (for the model tree / visibility toggles).
    pub node: String,
    /// RGBA.
    pub color: [f32; 4],
    /// xyz per corner, 9 floats per triangle.
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    /// Optional per-corner RGB (vertex colors).
    pub colors: Vec<f32>,
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

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 0.0 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// All world transforms of node `name` (one per path to the root; nodes
/// with several parents are instanced).
fn world_transforms(scene: &Scene, name: &str, depth: usize) -> Vec<Mat4> {
    if depth > 64 {
        return Vec::new(); // cycle or absurd nesting
    }
    let Some(node) = scene.nodes.iter().find(|n| n.name == name) else {
        return vec![IDENTITY];
    };
    let mut out = Vec::new();
    for p in &node.parents {
        if p.name.is_empty() {
            out.push(p.transform);
        } else {
            for w in world_transforms(scene, &p.name, depth + 1) {
                out.push(mul(&w, &p.transform));
            }
        }
    }
    if node.parents.is_empty() {
        out.push(IDENTITY);
    }
    out
}

const DEFAULT_COLOR: [f32; 4] = [0.75, 0.75, 0.75, 1.0];

pub fn flatten(scene: &Scene) -> Vec<Batch> {
    let mut out = Vec::new();
    for node in &scene.nodes {
        let NodeKind::Model {
            resource,
            visibility,
        } = &node.kind
        else {
            continue;
        };
        if *visibility == 0 {
            continue;
        }
        let Some(mesh) = scene.meshes.iter().find(|m| &m.name == resource) else {
            continue;
        };
        if mesh.faces.is_empty() {
            continue;
        }
        let shading_ids = mesh.faces.iter().map(|f| f.shading).max().unwrap_or(0) + 1;
        let color_of = |sid: u32| -> [f32; 4] {
            let shader = node.shading.get(sid as usize).and_then(|l| l.first());
            let material = shader
                .and_then(|s| scene.shaders.iter().find(|x| &x.name == s))
                .and_then(|s| scene.materials.iter().find(|m| m.name == s.material));
            match material {
                Some(m) => [
                    m.diffuse[0],
                    m.diffuse[1],
                    m.diffuse[2],
                    m.opacity.clamp(0.0, 1.0),
                ],
                None => DEFAULT_COLOR,
            }
        };
        for world in world_transforms(scene, &node.name, 0) {
            // Normals: inverse-transpose is overkill for the rigid
            // transforms U3D files use; apply the linear part and renormalize.
            let mut lin = world;
            lin[12] = 0.0;
            lin[13] = 0.0;
            lin[14] = 0.0;
            for sid in 0..shading_ids {
                let mut b = Batch {
                    node: node.name.clone(),
                    color: color_of(sid),
                    positions: Vec::new(),
                    normals: Vec::new(),
                    colors: Vec::new(),
                };
                for f in mesh.faces.iter().filter(|f| f.shading == sid) {
                    let p: Vec<[f32; 3]> = f
                        .position
                        .iter()
                        .map(|&i| apply(&world, mesh.positions[i as usize]))
                        .collect();
                    let flat = normalize(cross(sub(p[1], p[0]), sub(p[2], p[0])));
                    for c in 0..3 {
                        b.positions.extend_from_slice(&p[c]);
                        let n = f
                            .normal
                            .and_then(|n| mesh.normals.get(n[c] as usize))
                            .map(|n| normalize(apply(&lin, *n)))
                            .unwrap_or(flat);
                        b.normals.extend_from_slice(&n);
                        if let Some(d) = f.diffuse.and_then(|d| mesh.diffuse.get(d[c] as usize)) {
                            b.colors.extend_from_slice(&d[..3]);
                        }
                    }
                }
                if b.colors.len() != b.positions.len() {
                    b.colors.clear();
                }
                if !b.positions.is_empty() {
                    out.push(b);
                }
            }
        }
    }
    out
}
