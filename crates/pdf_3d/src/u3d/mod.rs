//! U3D (ECMA-363, Universal 3D File Format, 4th edition).

pub mod bits;
mod clod;
mod read;
mod write;

pub use read::parse;
pub use write::{write, ExportMesh, ExportScene};

use serde::Serialize;

pub(crate) mod block {
    pub const FILE_HEADER: u32 = 0x0044_3355;
    pub const FILE_REFERENCE: u32 = 0xFFFF_FF12;
    pub const MODIFIER_CHAIN: u32 = 0xFFFF_FF14;
    pub const PRIORITY_UPDATE: u32 = 0xFFFF_FF15;
    pub const GROUP_NODE: u32 = 0xFFFF_FF21;
    pub const MODEL_NODE: u32 = 0xFFFF_FF22;
    pub const LIGHT_NODE: u32 = 0xFFFF_FF23;
    pub const VIEW_NODE: u32 = 0xFFFF_FF24;
    pub const CLOD_MESH_DECLARATION: u32 = 0xFFFF_FF31;
    pub const POINT_SET_DECLARATION: u32 = 0xFFFF_FF36;
    pub const LINE_SET_DECLARATION: u32 = 0xFFFF_FF37;
    pub const CLOD_BASE_MESH: u32 = 0xFFFF_FF3B;
    pub const CLOD_PROGRESSIVE_MESH: u32 = 0xFFFF_FF3C;
    pub const POINT_SET_CONTINUATION: u32 = 0xFFFF_FF3E;
    pub const LINE_SET_CONTINUATION: u32 = 0xFFFF_FF3F;
    pub const SHADING_MODIFIER: u32 = 0xFFFF_FF45;
    pub const LIGHT_RESOURCE: u32 = 0xFFFF_FF51;
    pub const VIEW_RESOURCE: u32 = 0xFFFF_FF52;
    pub const LIT_TEXTURE_SHADER: u32 = 0xFFFF_FF53;
    pub const MATERIAL_RESOURCE: u32 = 0xFFFF_FF54;
    pub const TEXTURE_DECLARATION: u32 = 0xFFFF_FF55;
    pub const TEXTURE_CONTINUATION: u32 = 0xFFFF_FF5C;
}

/// Column-major 4x4 matrix as stored in U3D (translation in 12..15).
pub type Mat4 = [f32; 16];

pub const IDENTITY: Mat4 = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// A decoded U3D file.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Scene {
    /// Metres per file unit, when the file declares units.
    pub units_m: Option<f64>,
    pub nodes: Vec<Node>,
    pub meshes: Vec<Mesh>,
    pub shaders: Vec<Shader>,
    pub materials: Vec<Material>,
    pub lights: Vec<LightResource>,
    pub textures: Vec<Texture>,
    /// Things the decoder skipped (unsupported blocks, damaged data).
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub name: String,
    pub kind: NodeKind,
    pub parents: Vec<Parent>,
    /// Shader lists from the node's shading modifier: list `i` applies to
    /// shading ID `i` of the model's mesh; the first shader is used.
    pub shading: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Parent {
    /// Empty = the world (default group node).
    pub name: String,
    /// Transform relative to that parent.
    pub transform: Mat4,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum NodeKind {
    Group,
    Model { resource: String, visibility: u32 },
    Light { resource: String },
    View(ViewNode),
}

#[derive(Debug, Clone, Serialize)]
pub struct ViewNode {
    pub resource: String,
    pub attributes: u32,
    pub near: f32,
    pub far: f32,
    /// Field of view in degrees (perspective) or orthographic height.
    pub projection: f32,
}

/// Per-shading-ID description from the mesh declaration.
#[derive(Debug, Clone, Serialize)]
pub struct ShadingDesc {
    /// 1 = per-vertex diffuse colors, 2 = per-vertex specular colors.
    pub attributes: u32,
    pub texture_layers: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Face {
    pub shading: u32,
    pub position: [u32; 3],
    pub normal: Option<[u32; 3]>,
    pub diffuse: Option<[u32; 3]>,
    pub specular: Option<[u32; 3]>,
    /// First texture layer only.
    pub tex: Option<[u32; 3]>,
}

/// A CLOD mesh resource.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Mesh {
    pub name: String,
    pub exclude_normals: bool,
    pub shading: Vec<ShadingDesc>,
    /// Counts declared for the full-resolution mesh.
    pub max_faces: u32,
    pub max_positions: u32,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub diffuse: Vec<[f32; 4]>,
    pub specular: Vec<[f32; 4]>,
    pub tex_coords: Vec<[f32; 4]>,
    pub faces: Vec<Face>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Shader {
    pub name: String,
    pub attributes: u32,
    pub material: String,
    /// Texture names of the active texture layers.
    pub textures: Vec<String>,
}

/// A texture image (PNG or JPEG) from texture continuation blocks.
#[derive(Debug, Clone, Serialize)]
pub struct Texture {
    pub name: String,
    pub mime: String,
    #[serde(skip)]
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Material {
    pub name: String,
    pub attributes: u32,
    pub ambient: [f32; 3],
    pub diffuse: [f32; 3],
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    pub reflectivity: f32,
    pub opacity: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LightResource {
    pub name: String,
    pub attributes: u32,
    /// 0 ambient, 1 directional, 2 point, 3 spot.
    pub light_type: u8,
    pub color: [f32; 3],
    pub attenuation: [f32; 3],
    pub spot_angle: f32,
    pub intensity: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum U3dError {
    #[error("not a U3D file")]
    NotU3d,
    #[error("U3D data is damaged: {0}")]
    Damaged(String),
    #[error("U3D model too large: {0}")]
    TooLarge(String),
}
