//! U3D file parser: block walk, declaration blocks, CLOD meshes, textures.

use super::bits::{BitReader, STATIC_FULL};
use super::clod::Clod;
use super::*;

/// Upper bound for any element count in one mesh. Keeps a malformed or
/// hostile file from requesting gigabytes before the data runs out.
const MAX_ELEMENTS: u32 = 20_000_000;
/// Upper bound for one texture image.
const MAX_TEXTURE_BYTES: usize = 64 << 20;

/// ECMA-363 context for base mesh shading IDs (`cShading`).
const CONTEXT_BASE_SHADING_ID: u32 = 1;

struct RawBlock<'a> {
    block_type: u32,
    data: &'a [u8],
}

/// Walks the blocks of `bytes`.
fn blocks(bytes: &[u8]) -> impl Iterator<Item = Result<RawBlock<'_>, U3dError>> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        if pos + 12 > bytes.len() {
            return None;
        }
        let u = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let block_type = u(pos);
        let data_size = u(pos + 4) as usize;
        let meta_size = u(pos + 8) as usize;
        let start = pos + 12;
        let Some(end) = start.checked_add(data_size).filter(|e| *e <= bytes.len()) else {
            pos = bytes.len();
            return Some(Err(U3dError::Damaged(format!(
                "block {block_type:#010x} runs past the end of the file"
            ))));
        };
        let data = &bytes[start..end];
        pos = start
            .saturating_add(data_size.div_ceil(4) * 4)
            .saturating_add(meta_size.div_ceil(4) * 4);
        Some(Ok(RawBlock { block_type, data }))
    })
}

/// Decodes a U3D file (as stored in a PDF `/3D` stream with `/Subtype /U3D`).
///
/// Damage inside one mesh or texture is recorded in `Scene::warnings` and
/// the rest of the scene is still returned; only a broken file structure is
/// an error.
pub fn parse(bytes: &[u8]) -> Result<Scene, U3dError> {
    if !bytes.starts_with(&block::FILE_HEADER.to_le_bytes()) {
        return Err(U3dError::NotU3d);
    }
    let mut it = blocks(bytes);
    let header = it.next().ok_or(U3dError::NotU3d)??;
    if header.block_type != block::FILE_HEADER {
        return Err(U3dError::NotU3d);
    }
    let mut r = BitReader::new(header.data, false);
    let _version = r.u32();
    let profile = r.u32();
    let _declaration_size = r.u32();
    let _file_size = r.u64();
    let _encoding = r.u32();
    let units_m = (profile & 0x8 != 0)
        .then(|| r.f64())
        .filter(|u| u.is_finite() && *u > 0.0);

    let mut p = Parser {
        scene: Scene {
            units_m,
            ..Default::default()
        },
        no_compression: profile & 0x4 != 0,
        clods: Vec::new(),
        texture_parts: Vec::new(),
    };
    for b in it {
        let b = b?;
        p.block(&b, None)?;
    }
    p.finish();
    Ok(p.scene)
}

/// A texture declaration and the image data collected for it.
struct TextureParts {
    name: String,
    /// (compression type, channels, collected data) per continuation image.
    images: Vec<(u8, u8, Vec<u8>)>,
}

struct Parser {
    scene: Scene,
    no_compression: bool,
    /// Reconstruction state per entry of `scene.meshes`; `None` once the
    /// mesh was found damaged.
    clods: Vec<Option<Clod>>,
    texture_parts: Vec<TextureParts>,
}

impl Parser {
    fn reader<'a>(&self, data: &'a [u8]) -> BitReader<'a> {
        BitReader::new(data, self.no_compression)
    }

    fn warn(&mut self, msg: String) {
        if !self.scene.warnings.contains(&msg) {
            self.scene.warnings.push(msg);
        }
    }

    /// `chain_node`: index of the node declared earlier in the same
    /// modifier chain (shading modifiers attach to it).
    fn block(
        &mut self,
        b: &RawBlock,
        chain_node: Option<usize>,
    ) -> Result<Option<usize>, U3dError> {
        match b.block_type {
            block::MODIFIER_CHAIN => self.modifier_chain(b.data)?,
            block::PRIORITY_UPDATE | block::FILE_REFERENCE => {}
            block::GROUP_NODE | block::MODEL_NODE | block::LIGHT_NODE | block::VIEW_NODE => {
                return Ok(Some(self.node(b)?));
            }
            block::SHADING_MODIFIER => self.shading_modifier(b.data, chain_node)?,
            block::CLOD_MESH_DECLARATION => self.mesh_declaration(b.data)?,
            block::CLOD_BASE_MESH => self.mesh_continuation(b.data, false),
            block::CLOD_PROGRESSIVE_MESH => self.mesh_continuation(b.data, true),
            block::LIT_TEXTURE_SHADER => self.shader(b.data)?,
            block::MATERIAL_RESOURCE => self.material(b.data)?,
            block::LIGHT_RESOURCE => self.light(b.data)?,
            block::VIEW_RESOURCE => {}
            block::TEXTURE_DECLARATION => self.texture_declaration(b.data),
            block::TEXTURE_CONTINUATION => self.texture_continuation(b.data),
            block::POINT_SET_DECLARATION
            | block::POINT_SET_CONTINUATION
            | block::LINE_SET_DECLARATION
            | block::LINE_SET_CONTINUATION => self.warn("point and line sets not shown".into()),
            other => self.warn(format!("block {other:#010x} skipped")),
        }
        Ok(None)
    }

    fn modifier_chain(&mut self, data: &[u8]) -> Result<(), U3dError> {
        let mut r = self.reader(data);
        let _name = r.string();
        let _chain_type = r.u32();
        let attributes = r.u32();
        if attributes & 1 != 0 {
            for _ in 0..4 {
                r.f32();
            }
        }
        if attributes & 2 != 0 {
            for _ in 0..6 {
                r.f32();
            }
        }
        // Padding to 32-bit alignment, then the modifier count.
        let pos = r.byte_position().div_ceil(4) * 4;
        if pos + 4 > data.len() {
            return Err(U3dError::Damaged("modifier chain header".into()));
        }
        let count = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        let mut node = None;
        for (i, b) in blocks(&data[pos + 4..]).enumerate() {
            if i as u32 >= count {
                break;
            }
            let b = b?;
            if let Some(n) = self.block(&b, node)? {
                node = Some(n);
            }
        }
        Ok(())
    }

    fn parents(r: &mut BitReader) -> Vec<Parent> {
        let n = r.u32();
        let mut out = Vec::new();
        for _ in 0..n {
            if r.overrun() {
                break;
            }
            let name = r.string();
            let mut transform = [0f32; 16];
            for v in &mut transform {
                *v = r.f32();
            }
            out.push(Parent { name, transform });
        }
        out
    }

    fn node(&mut self, b: &RawBlock) -> Result<usize, U3dError> {
        let mut r = self.reader(b.data);
        let name = r.string();
        let parents = Self::parents(&mut r);
        let kind = match b.block_type {
            block::GROUP_NODE => NodeKind::Group,
            block::MODEL_NODE => {
                let resource = r.string();
                let visibility = r.u32();
                NodeKind::Model {
                    resource,
                    visibility,
                }
            }
            block::LIGHT_NODE => NodeKind::Light {
                resource: r.string(),
            },
            _ => {
                let resource = r.string();
                let attributes = r.u32();
                let near = r.f32();
                let far = r.f32();
                let projection = r.f32();
                NodeKind::View(ViewNode {
                    resource,
                    attributes,
                    near,
                    far,
                    projection,
                })
            }
        };
        if r.overrun() {
            return Err(U3dError::Damaged(format!("node {name}")));
        }
        self.scene.nodes.push(Node {
            name,
            kind,
            parents,
            shading: Vec::new(),
        });
        Ok(self.scene.nodes.len() - 1)
    }

    fn shading_modifier(&mut self, data: &[u8], chain_node: Option<usize>) -> Result<(), U3dError> {
        let mut r = self.reader(data);
        let _name = r.string();
        let _chain_index = r.u32();
        let _attributes = r.u32();
        let lists = r.u32();
        let mut shading = Vec::new();
        for _ in 0..lists {
            let n = r.u32();
            let mut list = Vec::new();
            for _ in 0..n {
                if r.overrun() {
                    return Err(U3dError::Damaged("shading modifier".into()));
                }
                list.push(r.string());
            }
            shading.push(list);
        }
        match chain_node {
            Some(i) => self.scene.nodes[i].shading = shading,
            None => self.warn("shading modifier without a node".into()),
        }
        Ok(())
    }

    fn mesh_declaration(&mut self, data: &[u8]) -> Result<(), U3dError> {
        let mut r = self.reader(data);
        let name = r.string();
        let _chain_index = r.u32();
        let attributes = r.u32();
        let max_faces = r.u32();
        let max_positions = r.u32();
        let _normals = r.u32();
        let _diffuse = r.u32();
        let _specular = r.u32();
        let _tex = r.u32();
        let shading_count = r.u32();
        if shading_count > 0xFFFF || max_faces > MAX_ELEMENTS || max_positions > MAX_ELEMENTS {
            return Err(U3dError::TooLarge(format!("mesh {name}")));
        }
        let mut shading = Vec::new();
        for _ in 0..shading_count {
            let attributes = r.u32();
            let texture_layers = r.u32();
            if texture_layers > 8 {
                return Err(U3dError::Damaged(format!(
                    "mesh {name}: {texture_layers} texture layers"
                )));
            }
            for _ in 0..texture_layers {
                r.u32();
            }
            let _original_id = r.u32();
            shading.push(ShadingDesc {
                attributes,
                texture_layers,
            });
        }
        let _min_resolution = r.u32();
        let final_resolution = r.u32();
        for _ in 0..3 {
            r.u32(); // quality factors
        }
        let mut inverse_quant = [0f32; 5];
        for q in &mut inverse_quant {
            *q = r.f32();
        }
        // Normal crease / update / tolerance and the skeleton follow; the
        // viewer needs neither.
        if r.overrun() {
            return Err(U3dError::Damaged(format!("mesh declaration {name}")));
        }
        let exclude_normals = attributes & 1 != 0;
        self.clods.push(Some(Clod::new(
            exclude_normals,
            shading.clone(),
            inverse_quant,
            final_resolution.max(max_positions),
            max_faces,
        )));
        self.scene.meshes.push(Mesh {
            name,
            exclude_normals,
            shading,
            max_faces,
            max_positions: final_resolution.max(max_positions),
            ..Default::default()
        });
        Ok(())
    }

    /// Base or progressive mesh continuation. Damage is a warning: the mesh
    /// keeps what was decoded before.
    fn mesh_continuation(&mut self, data: &[u8], progressive: bool) {
        let mut r = self.reader(data);
        let name = r.string();
        let _chain_index = r.u32();
        let Some(mi) = self.scene.meshes.iter().position(|m| m.name == name) else {
            self.warn(format!("mesh data for undeclared mesh {name}"));
            return;
        };
        let Some(mut clod) = self.clods[mi].take() else {
            return;
        };
        let result = if progressive {
            clod.progressive(&mut r)
        } else {
            self.base_mesh(&mut r, &name, &mut clod)
        };
        match result {
            Ok(()) => self.clods[mi] = Some(clod),
            Err(e) => {
                self.warn(format!("mesh {name}: {e}"));
                // Keep what was decoded so far.
                clod.finish(&mut self.scene.meshes[mi]);
            }
        }
    }

    fn base_mesh(
        &mut self,
        r: &mut BitReader,
        name: &str,
        clod: &mut Clod,
    ) -> Result<(), U3dError> {
        let counts: Vec<u32> = (0..6).map(|_| r.u32()).collect();
        let (faces, positions, normals, diffuse, specular, tex) = (
            counts[0], counts[1], counts[2], counts[3], counts[4], counts[5],
        );
        if counts.iter().any(|c| *c > MAX_ELEMENTS) {
            return Err(U3dError::TooLarge(format!("base mesh {name}")));
        }
        let shading = clod.shading.clone();
        let exclude_normals = clod.exclude_normals;
        let layers = shading
            .iter()
            .map(|s| s.texture_layers as usize)
            .max()
            .unwrap_or(0);

        let v3 = |r: &mut BitReader| [r.f32(), r.f32(), r.f32()];
        let v4 = |r: &mut BitReader| [r.f32(), r.f32(), r.f32(), r.f32()];
        let read3 = |r: &mut BitReader, n: u32| -> Result<Vec<[f32; 3]>, U3dError> {
            let mut v = Vec::new();
            for _ in 0..n {
                if r.overrun() {
                    return Err(U3dError::Damaged(format!(
                        "base mesh {name}: data ends early"
                    )));
                }
                v.push(v3(r));
            }
            Ok(v)
        };
        let read4 = |r: &mut BitReader, n: u32| -> Result<Vec<[f32; 4]>, U3dError> {
            let mut v = Vec::new();
            for _ in 0..n {
                if r.overrun() {
                    return Err(U3dError::Damaged(format!(
                        "base mesh {name}: data ends early"
                    )));
                }
                v.push(v4(r));
            }
            Ok(v)
        };
        let p = read3(r, positions)?;
        let n = read3(r, normals)?;
        let d = read4(r, diffuse)?;
        let s = read4(r, specular)?;
        let t = read4(r, tex)?;

        let mut f = Vec::new();
        let mut tex_faces: Vec<Vec<[u32; 3]>> = vec![Vec::new(); layers];
        for _ in 0..faces {
            if r.overrun() {
                return Err(U3dError::Damaged(format!("base mesh {name}: faces")));
            }
            let sid = r.compressed_u32(CONTEXT_BASE_SHADING_ID);
            let desc = shading.get(sid as usize).cloned().unwrap_or(ShadingDesc {
                attributes: 0,
                texture_layers: 0,
            });
            let mut face = Face {
                shading: sid,
                ..Default::default()
            };
            let mut nrm = [0u32; 3];
            let mut dif = [0u32; 3];
            let mut spe = [0u32; 3];
            let mut tx = vec![[0u32; 3]; layers];
            for c in 0..3 {
                face.position[c] = r.compressed_u32(STATIC_FULL + positions);
                if !exclude_normals {
                    nrm[c] = r.compressed_u32(STATIC_FULL + normals);
                }
                if desc.attributes & 1 != 0 {
                    dif[c] = r.compressed_u32(STATIC_FULL + diffuse);
                }
                if desc.attributes & 2 != 0 {
                    spe[c] = r.compressed_u32(STATIC_FULL + specular);
                }
                for layer in 0..desc.texture_layers as usize {
                    let i = r.compressed_u32(STATIC_FULL + tex);
                    if let Some(l) = tx.get_mut(layer) {
                        l[c] = i;
                    }
                }
            }
            if face.position.iter().any(|i| *i >= positions) {
                return Err(U3dError::Damaged(format!(
                    "base mesh {name}: position index"
                )));
            }
            face.normal = (!exclude_normals).then_some(nrm);
            face.diffuse = (desc.attributes & 1 != 0).then_some(dif);
            face.specular = (desc.attributes & 2 != 0).then_some(spe);
            for (l, v) in tx.into_iter().enumerate() {
                tex_faces[l].push(v);
            }
            f.push(face);
        }
        clod.set_base(p, n, d, s, t, f, tex_faces)
    }

    fn shader(&mut self, data: &[u8]) -> Result<(), U3dError> {
        let mut r = self.reader(data);
        let name = r.string();
        let attributes = r.u32();
        let _alpha_ref = r.f32();
        let _alpha_fn = r.u32();
        let _blend_fn = r.u32();
        let _passes = r.u32();
        let channels = r.u32();
        let _alpha_channels = r.u32();
        let material = r.string();
        let mut layers = Vec::new();
        for channel in (0..8).filter(|c| channels & (1 << c) != 0) {
            if r.overrun() {
                break;
            }
            let texture = r.string();
            let intensity = r.f32();
            let blend_function = r.u8();
            let blend_source = r.u8();
            let blend_constant = r.f32();
            let mode = r.u8();
            let mut transform = [0f32; 16];
            let mut wrap_transform = [0f32; 16];
            for v in transform.iter_mut().chain(wrap_transform.iter_mut()) {
                *v = r.f32();
            }
            let repeat = r.u8();
            layers.push(TextureLayer {
                channel,
                texture,
                intensity,
                blend_function,
                blend_source,
                blend_constant,
                mode,
                transform,
                wrap_transform,
                repeat,
            });
        }
        self.scene.shaders.push(Shader {
            name,
            attributes,
            material,
            layers,
        });
        Ok(())
    }

    fn material(&mut self, data: &[u8]) -> Result<(), U3dError> {
        let mut r = self.reader(data);
        let name = r.string();
        let attributes = r.u32();
        let mut c3 = || [r.f32(), r.f32(), r.f32()];
        let ambient = c3();
        let diffuse = c3();
        let specular = c3();
        let emissive = c3();
        let reflectivity = r.f32();
        let opacity = r.f32();
        self.scene.materials.push(Material {
            name,
            attributes,
            ambient,
            diffuse,
            specular,
            emissive,
            reflectivity,
            opacity,
        });
        Ok(())
    }

    fn light(&mut self, data: &[u8]) -> Result<(), U3dError> {
        let mut r = self.reader(data);
        let name = r.string();
        let attributes = r.u32();
        let light_type = r.u8();
        let color = [r.f32(), r.f32(), r.f32()];
        let _reserved = r.f32();
        let attenuation = [r.f32(), r.f32(), r.f32()];
        let spot_angle = r.f32();
        let intensity = r.f32();
        self.scene.lights.push(LightResource {
            name,
            attributes,
            light_type,
            color,
            attenuation,
            spot_angle,
            intensity,
        });
        Ok(())
    }

    fn texture_declaration(&mut self, data: &[u8]) {
        let mut r = self.reader(data);
        let name = r.string();
        let _height = r.u32();
        let _width = r.u32();
        let _image_type = r.u8();
        let count = r.u32().min(16);
        let mut images = Vec::new();
        for _ in 0..count {
            let compression = r.u8();
            let channels = r.u8();
            let attributes = r.u16();
            if attributes & 1 != 0 {
                let urls = r.u32().min(64);
                for _ in 0..urls {
                    r.string();
                }
                self.warn(format!(
                    "texture {name}: external image files are not loaded"
                ));
            } else {
                r.u32(); // byte count
            }
            images.push((compression, channels, Vec::new()));
        }
        self.texture_parts.retain(|t| t.name != name);
        self.texture_parts.push(TextureParts { name, images });
    }

    fn texture_continuation(&mut self, data: &[u8]) {
        let mut r = self.reader(data);
        let name = r.string();
        let index = r.u32() as usize;
        // Image data is raw bytes after the two fields (byte-aligned: no
        // compressed value precedes them).
        let start = r.byte_position().min(data.len());
        let Some(t) = self.texture_parts.iter_mut().find(|t| t.name == name) else {
            return;
        };
        if let Some(img) = t.images.get_mut(index) {
            if img.2.len() + data.len() - start <= MAX_TEXTURE_BYTES {
                img.2.extend_from_slice(&data[start..]);
            }
        }
    }

    fn finish(&mut self) {
        for (mi, clod) in self.clods.iter_mut().enumerate() {
            if let Some(c) = clod.take() {
                c.finish(&mut self.scene.meshes[mi]);
            }
        }
        for t in std::mem::take(&mut self.texture_parts) {
            // The color image is the one carrying red (or luminance); alpha
            // images composed on top are not applied.
            let color = t
                .images
                .iter()
                .find(|(_, ch, d)| ch & 0x18 != 0 && !d.is_empty())
                .or_else(|| t.images.iter().find(|(_, _, d)| !d.is_empty()));
            let Some((compression, _, bytes)) = color else {
                continue;
            };
            let mime = match compression {
                1 | 3 => "image/jpeg",
                2 => "image/png",
                _ => {
                    self.scene
                        .warnings
                        .push(format!("texture {}: TIFF images are not shown", t.name));
                    continue;
                }
            };
            if t.images.len() > 1 {
                self.warn(format!("texture {}: only the color image is used", t.name));
            }
            self.scene.textures.push(Texture {
                name: t.name,
                mime: mime.into(),
                data: bytes.clone(),
            });
        }
    }
}
