//! CLOD mesh reconstruction: base mesh plus progressive mesh continuation
//! blocks (ECMA-363 9.6.1.3).
//!
//! A progressive mesh is a sequence of vertex splits. Each resolution
//! update adds one position: a "split" position is chosen, new faces are
//! formed from the split position, the new position and a third position,
//! and some faces using the split position "move" over to the new one.
//! Almost everything is coded relative to sets derived from the current
//! mesh (faces around a position, positions around those faces, colors at
//! a position), so the decoder has to rebuild those sets exactly as the
//! encoder saw them.
//!
//! This follows the decoder of the Universal 3D Sample Software
//! (`CIFXAuthorCLODDecoder_P.cpp`, Copyright Intel Corporation, Apache
//! License 2.0), including its conventions that the standard leaves open:
//! sets are ordered by descending value, and the static context
//! `STATIC_FULL + 0` behaves like a dynamic context (handled in `bits`).
//! It reconstructs only the final resolution, so face updates are applied
//! directly instead of being stored for later resolution changes.
//! License notice: `crates/pdf_3d/licenses/README.md`.

use super::bits::{BitReader, STATIC_FULL};
use super::{Face, Mesh, ShadingDesc, U3dError};

// Contexts (IFXACContext.h).
const CTX_NUM_NEW_FACES: u32 = 1;
const CTX_ORIENTATION: u32 = 2;
const CTX_THIRD_INDEX_TYPE: u32 = 3;
const CTX_LOCAL_3RD_POSITION: u32 = 4;
const CTX_STAY_MOVE: u32 = 15;
const CTX_POSITION_SIGNS: u32 = 20;
const CTX_POSITION_MAG: [u32; 3] = [21, 22, 23];
const CTX_TEX_SPLIT_TYPE: u32 = 29;
const CTX_TEX_DIFF_MAG: [u32; 4] = [33, 34, 35, 36];
const CTX_TEX_DUP_TYPE: u32 = 39;
const CTX_NUM_LOCAL_NORMALS: u32 = 40;
const CTX_NORMAL_SIGNS: u32 = 41;
const CTX_NORMAL_MAG: [u32; 3] = [42, 43, 44];
const CTX_NORMAL_LOCAL_INDEX: u32 = 45;
const CTX_COLOR_SPLIT_TYPE: u32 = 55;
const CTX_COLOR_DUP_TYPE: u32 = 56;
const CTX_COLOR_DIFF_MAG: [u32; 4] = [60, 61, 62, 63];
const CTX_SHADING_ID: u32 = 65;
const CTX_NEW_DIFFUSE_COUNT: u32 = 99;
const CTX_DIFFUSE_SIGN: u32 = 100;
const CTX_NEW_SPECULAR_COUNT: u32 = 101;
const CTX_SPECULAR_SIGN: u32 = 102;
const CTX_TEX_SIGN: u32 = 103;
/// Keep/change, change type, new/local/global index — per attribute.
const CTX_DIFFUSE_CHANGE: [u32; 5] = [104, 105, 106, 107, 108];
const CTX_SPECULAR_CHANGE: [u32; 5] = [109, 110, 111, 112, 113];
const CTX_TEX_CHANGE: [u32; 5] = [114, 115, 116, 117, 118];
const CTX_COLOR_INDEX_LOCAL: u32 = 119;
const CTX_COLOR_INDEX_GLOBAL: u32 = 120;
const CTX_TEX_INDEX_LOCAL: u32 = 121;
const CTX_TEX_INDEX_GLOBAL: u32 = 122;
const CTX_NEW_TEX_COUNT: u32 = 123;

const ORIENT_LEFT: u8 = 1;
const THIRD_LOCAL: u8 = 1;
const PREDICT_NONE: u32 = 0;
const PREDICT_MOVE: u32 = 1;
const PREDICT_STAY: u32 = 2;
const PREDICT_MOVE2: u32 = 3;
const PREDICT_STAY2: u32 = 4;
const MOVE: u8 = 1;
const UPDATE_CHANGE: u8 = 1;
const UPDATE_NEW: u8 = 1;
const UPDATE_LOCAL: u8 = 2;
const SPLIT_DUP: u8 = 1;
const UPDATE_DUP: u8 = 2;
const THIRD_DUP: u8 = 4;

/// Hard cap on faces around one position: the reference keeps these in
/// small arrays; a hostile file must not make every update quadratic.
const MAX_FAN: usize = 65_536;

fn damaged(msg: impl Into<String>) -> U3dError {
    U3dError::Damaged(msg.into())
}

/// A set of indices ordered by descending value (IFXSetX).
#[derive(Debug, Clone, Default)]
struct DescSet(Vec<u32>);

impl DescSet {
    fn pos(&self, v: u32) -> Result<usize, usize> {
        self.0.binary_search_by(|x| v.cmp(x))
    }
    fn add(&mut self, v: u32) {
        if let Err(i) = self.pos(v) {
            self.0.insert(i, v);
        }
    }
    fn remove(&mut self, v: u32) {
        if let Ok(i) = self.pos(v) {
            self.0.remove(i);
        }
    }
    fn contains(&self, v: u32) -> bool {
        self.pos(v).is_ok()
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn get(&self, i: u32) -> Result<u32, U3dError> {
        self.0
            .get(i as usize)
            .copied()
            .ok_or_else(|| damaged("set index out of range"))
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Attr {
    Diffuse,
    Specular,
    Tex(usize),
}

/// Mesh under reconstruction.
#[derive(Debug, Default)]
pub(crate) struct Clod {
    pub exclude_normals: bool,
    pub shading: Vec<ShadingDesc>,
    /// Position, normal, texture coordinate, diffuse, specular.
    pub inverse_quant: [f32; 5],
    pub final_resolution: u32,
    pub max_faces: u32,

    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    diffuse: Vec<[f32; 4]>,
    specular: Vec<[f32; 4]>,
    tex: Vec<[f32; 4]>,

    f_mat: Vec<u32>,
    f_pos: Vec<[u32; 3]>,
    f_nrm: Vec<[u32; 3]>,
    f_dif: Vec<[u32; 3]>,
    f_spe: Vec<[u32; 3]>,
    /// Per texture layer, one corner triple per face.
    f_tex: Vec<Vec<[u32; 3]>>,

    /// Faces using each position.
    adj: Vec<DescSet>,
    /// Previous split/update/third indices: diffuse, specular, texture.
    prev: [[u32; 3]; 3],
}

impl Clod {
    pub fn new(
        exclude_normals: bool,
        shading: Vec<ShadingDesc>,
        inverse_quant: [f32; 5],
        final_resolution: u32,
        max_faces: u32,
    ) -> Self {
        let layers = shading
            .iter()
            .map(|s| s.texture_layers as usize)
            .max()
            .unwrap_or(0);
        Self {
            exclude_normals,
            shading,
            inverse_quant,
            final_resolution,
            max_faces,
            f_tex: vec![Vec::new(); layers],
            ..Default::default()
        }
    }

    pub fn resolution(&self) -> u32 {
        self.positions.len() as u32
    }

    fn material(&self, face: u32) -> (bool, bool, usize) {
        match self
            .f_mat
            .get(face as usize)
            .and_then(|m| self.shading.get(*m as usize))
        {
            Some(s) => (
                s.attributes & 1 != 0,
                s.attributes & 2 != 0,
                s.texture_layers as usize,
            ),
            None => (false, false, 0),
        }
    }

    fn push_face(&mut self, material: u32, pos: [u32; 3]) {
        self.f_mat.push(material);
        self.f_pos.push(pos);
        self.f_nrm.push([0; 3]);
        self.f_dif.push([0; 3]);
        self.f_spe.push([0; 3]);
        for layer in &mut self.f_tex {
            layer.push([0; 3]);
        }
    }

    fn adj_mut(&mut self, pos: u32) -> Result<&mut DescSet, U3dError> {
        let i = pos as usize;
        if i > self.final_resolution as usize {
            return Err(damaged("position index beyond the declared resolution"));
        }
        if self.adj.len() <= i {
            self.adj.resize(i + 1, DescSet::default());
        }
        Ok(&mut self.adj[i])
    }

    fn faces_at(&self, pos: u32) -> DescSet {
        self.adj.get(pos as usize).cloned().unwrap_or_default()
    }

    fn link(&mut self, face: u32) -> Result<(), U3dError> {
        for p in self.f_pos[face as usize] {
            self.adj_mut(p)?.add(face);
        }
        Ok(())
    }

    /// Positions used by `faces` (IFXSetAdjacencyX::GetPositionSetX).
    fn positions_of(&self, faces: &DescSet) -> DescSet {
        let mut s = DescSet::default();
        for &f in &faces.0 {
            for p in self.f_pos[f as usize] {
                s.add(p);
            }
        }
        s
    }

    /// Attribute indices used at `pos` by faces whose shading has the
    /// attribute (GetDiffuseColorSetX and friends).
    fn attr_set(&self, attr: Attr, pos: u32) -> DescSet {
        let mut s = DescSet::default();
        let Some(faces) = self.adj.get(pos as usize) else {
            return s;
        };
        for &f in &faces.0 {
            let (dif, spe, layers) = self.material(f);
            let fi = f as usize;
            let corners = match attr {
                Attr::Diffuse if dif => self.f_dif[fi],
                Attr::Specular if spe => self.f_spe[fi],
                Attr::Tex(l) if l < layers && l < self.f_tex.len() => self.f_tex[l][fi],
                _ => continue,
            };
            if let Some(c) = self.f_pos[fi].iter().position(|p| *p == pos) {
                s.add(corners[c]);
            }
        }
        s
    }

    /// Fills the base mesh (CLOD Base Mesh Continuation).
    #[allow(clippy::too_many_arguments)]
    pub fn set_base(
        &mut self,
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        diffuse: Vec<[f32; 4]>,
        specular: Vec<[f32; 4]>,
        tex: Vec<[f32; 4]>,
        faces: Vec<Face>,
        tex_layers: Vec<Vec<[u32; 3]>>,
    ) -> Result<(), U3dError> {
        self.positions = positions;
        self.normals = normals;
        self.diffuse = diffuse;
        self.specular = specular;
        self.tex = tex;
        for (i, f) in faces.iter().enumerate() {
            self.push_face(f.shading, f.position);
            self.f_nrm[i] = f.normal.unwrap_or_default();
            self.f_dif[i] = f.diffuse.unwrap_or_default();
            self.f_spe[i] = f.specular.unwrap_or_default();
            for (l, layer) in self.f_tex.iter_mut().enumerate() {
                if let Some(c) = tex_layers.get(l).and_then(|t| t.get(i)) {
                    layer[i] = *c;
                }
            }
        }
        for i in 0..faces.len() {
            self.link(i as u32)?;
        }
        Ok(())
    }

    /// Decodes one CLOD Progressive Mesh Continuation block (after the
    /// mesh name and chain index).
    pub fn progressive(&mut self, r: &mut BitReader) -> Result<(), U3dError> {
        let start = r.u32();
        let end = r.u32();
        if start != self.resolution() {
            return Err(damaged(format!(
                "progressive block starts at resolution {start}, mesh is at {}",
                self.resolution()
            )));
        }
        if end > self.final_resolution {
            return Err(damaged("progressive block exceeds the declared resolution"));
        }
        self.prev = [[0; 3]; 3];
        for _ in start..end {
            if r.overrun() {
                return Err(damaged("progressive mesh data ends early"));
            }
            self.update(r)?;
            if self.f_pos.len() as u64 > self.max_faces as u64 + 16 {
                return Err(damaged("more faces than declared"));
            }
        }
        Ok(())
    }

    fn read_attr_values(
        &self,
        r: &mut BitReader,
        count: u16,
        predicted: [f32; 4],
        sign_ctx: u32,
        mag_ctx: [u32; 4],
        inv: f32,
    ) -> Vec<[f32; 4]> {
        (0..count)
            .map(|_| {
                let signs = r.compressed_u8(sign_ctx);
                let mut v = [0f32; 4];
                for (k, c) in v.iter_mut().enumerate() {
                    let m = r.compressed_u32(mag_ctx[k]) as f32 * inv;
                    *c = if signs & (1 << k) != 0 { -m } else { m } + predicted[k];
                }
                v
            })
            .collect()
    }

    fn average(values: &[[f32; 4]], set: &DescSet) -> [f32; 4] {
        let mut acc = [0f32; 4];
        let mut n = 0;
        for &i in &set.0 {
            if let Some(v) = values.get(i as usize) {
                for k in 0..4 {
                    acc[k] += v[k];
                }
            }
            n += 1;
        }
        if n > 0 {
            acc.iter_mut().for_each(|a| *a /= n as f32);
        }
        acc
    }

    /// One resolution update (vertex split).
    fn update(&mut self, r: &mut BitReader) -> Result<(), U3dError> {
        let cur = self.resolution();
        let [iq_pos, _iq_nrm, iq_tex, iq_dif, iq_spe] = self.inverse_quant;

        let split = r.compressed_u32(STATIC_FULL + cur);
        if cur > 0 && split >= cur {
            return Err(damaged("split position out of range"));
        }
        let split_faces = self.faces_at(split);
        if split_faces.len() > MAX_FAN {
            return Err(U3dError::TooLarge("vertex fan".into()));
        }
        let mut third_candidates = self.positions_of(&split_faces);
        third_candidates.remove(split);

        // New vertex attributes, predicted from the values at the split
        // position.
        let n = r.compressed_u16(CTX_NEW_DIFFUSE_COUNT);
        let diffuse_before = self.diffuse.len() as u32;
        if n > 0 {
            let p = Self::average(&self.diffuse, &self.attr_set(Attr::Diffuse, split));
            let v = self.read_attr_values(r, n, p, CTX_DIFFUSE_SIGN, CTX_COLOR_DIFF_MAG, iq_dif);
            self.diffuse.extend(v);
        }
        let n = r.compressed_u16(CTX_NEW_SPECULAR_COUNT);
        let specular_before = self.specular.len() as u32;
        if n > 0 {
            let p = Self::average(&self.specular, &self.attr_set(Attr::Specular, split));
            let v = self.read_attr_values(r, n, p, CTX_SPECULAR_SIGN, CTX_COLOR_DIFF_MAG, iq_spe);
            self.specular.extend(v);
        }
        let n = r.compressed_u16(CTX_NEW_TEX_COUNT);
        let tex_before = self.tex.len() as u32;
        if n > 0 {
            let p = Self::average(&self.tex, &self.attr_set(Attr::Tex(0), split));
            let v = self.read_attr_values(r, n, p, CTX_TEX_SIGN, CTX_TEX_DIFF_MAG, iq_tex);
            self.tex.extend(v);
        }

        // New faces: split position, new position, third position.
        let new_faces = r.compressed_u32(CTX_NUM_NEW_FACES);
        if new_faces as usize > MAX_FAN {
            return Err(U3dError::TooLarge("new faces in one update".into()));
        }
        let faces_before = self.f_pos.len() as u32;
        let mut left = DescSet::default();
        let mut right = DescSet::default();
        for _ in 0..new_faces {
            let shading = r.compressed_u32(CTX_SHADING_ID);
            let orientation = r.compressed_u8(CTX_ORIENTATION);
            let third_type = r.compressed_u8(CTX_THIRD_INDEX_TYPE);
            let third = if third_type == THIRD_LOCAL {
                let i = r.compressed_u32(CTX_LOCAL_3RD_POSITION);
                third_candidates.get(i)?
            } else {
                let t = r.compressed_u32(STATIC_FULL + cur);
                if t >= cur.max(1) {
                    return Err(damaged("third position out of range"));
                }
                third_candidates.add(t);
                t
            };
            let pos = if orientation == ORIENT_LEFT {
                left.add(third);
                [split, cur, third]
            } else {
                right.add(third);
                [cur, split, third]
            };
            self.push_face(shading, pos);
        }

        // Faces around the split position either stay or move to the new
        // position; the context depends on a prediction from the new faces.
        let mut move_faces = DescSet::default();
        let mut move_positions = DescSet::default();
        let mut stay_positions = DescSet::default();
        for &f in &split_faces.0 {
            let p = self.f_pos[f as usize];
            let Some(c) = p.iter().position(|x| *x == split) else {
                return Err(damaged("adjacency out of sync"));
            };
            let (next, prev) = (p[(c + 1) % 3], p[(c + 2) % 3]);
            let mut prediction = if right.contains(next) {
                PREDICT_MOVE
            } else if right.contains(prev) || left.contains(next) {
                PREDICT_STAY
            } else if left.contains(prev) {
                PREDICT_MOVE
            } else {
                PREDICT_NONE
            };
            if prediction == PREDICT_NONE && p.iter().any(|x| move_positions.contains(*x)) {
                prediction = PREDICT_MOVE2;
            }
            if prediction == PREDICT_NONE && p.iter().any(|x| stay_positions.contains(*x)) {
                prediction = PREDICT_STAY2;
            }
            let set = if r.compressed_u8(CTX_STAY_MOVE + prediction) == MOVE {
                move_faces.add(f);
                &mut move_positions
            } else {
                &mut stay_positions
            };
            for x in p {
                set.add(x);
            }
            set.remove(split);
        }

        // Attribute changes of moving faces. Recorded and applied together
        // with the position change once the new position exists.
        struct Change {
            attr: Option<Attr>,
            face: usize,
            corner: usize,
            value: u32,
        }
        let mut changes: Vec<Change> = Vec::new();
        for &f in &move_faces.0 {
            let fi = f as usize;
            let corner = self.f_pos[fi].iter().position(|x| *x == split).unwrap_or(0);
            changes.push(Change {
                attr: None,
                face: fi,
                corner,
                value: cur,
            });
            let (dif, spe, layers) = self.material(f);
            let mut attrs: Vec<(Attr, [u32; 5], u32)> = Vec::new();
            if dif {
                attrs.push((Attr::Diffuse, CTX_DIFFUSE_CHANGE, diffuse_before));
            }
            if spe {
                attrs.push((Attr::Specular, CTX_SPECULAR_CHANGE, specular_before));
            }
            for l in 0..layers.min(self.f_tex.len()) {
                attrs.push((Attr::Tex(l), CTX_TEX_CHANGE, tex_before));
            }
            for (attr, ctx, before) in attrs {
                if r.compressed_u8(ctx[0]) != UPDATE_CHANGE {
                    continue;
                }
                let kind = r.compressed_u8(ctx[1]);
                let value = if kind == UPDATE_NEW {
                    r.compressed_u32(ctx[2]) + before
                } else if kind == UPDATE_LOCAL {
                    let i = r.compressed_u32(ctx[3]);
                    self.attr_set(attr, split).get(i)?
                } else {
                    r.compressed_u32(ctx[4])
                };
                changes.push(Change {
                    attr: Some(attr),
                    face: fi,
                    corner,
                    value,
                });
            }
        }

        // Attribute indices of the new faces.
        for j in 0..new_faces {
            let fi = (faces_before + j) as usize;
            let p = self.f_pos[fi];
            // Corners as built above: [split, new, third] or [new, split, third].
            let (split_c, new_c, third_c) = if p[0] == split && p[1] == cur {
                (0, 1, 2)
            } else {
                (1, 0, 2)
            };
            let third = p[third_c];
            let (dif, spe, layers) = self.material(fi as u32);
            let mut attrs: Vec<(Attr, usize)> = Vec::new();
            if dif {
                attrs.push((Attr::Diffuse, 0));
            }
            if spe {
                attrs.push((Attr::Specular, 1));
            }
            for l in 0..layers.min(self.f_tex.len()) {
                attrs.push((Attr::Tex(l), 2));
            }
            for (attr, prev_slot) in attrs {
                let is_tex = matches!(attr, Attr::Tex(_));
                let (dup_ctx, type_ctx, local_ctx, global_ctx) = if is_tex {
                    (
                        CTX_TEX_DUP_TYPE,
                        CTX_TEX_SPLIT_TYPE,
                        CTX_TEX_INDEX_LOCAL,
                        CTX_TEX_INDEX_GLOBAL,
                    )
                } else {
                    (
                        CTX_COLOR_DUP_TYPE,
                        CTX_COLOR_SPLIT_TYPE,
                        CTX_COLOR_INDEX_LOCAL,
                        CTX_COLOR_INDEX_GLOBAL,
                    )
                };
                let dup = r.compressed_u8(dup_ctx);
                let split_set = self.attr_set(attr, split);
                let read = |r: &mut BitReader,
                            set: &DescSet,
                            prev: u32,
                            flag: u8|
                 -> Result<u32, U3dError> {
                    if dup & flag != 0 {
                        return Ok(prev);
                    }
                    if r.compressed_u8(type_ctx) == UPDATE_LOCAL {
                        set.get(r.compressed_u32(local_ctx))
                    } else {
                        Ok(r.compressed_u32(global_ctx))
                    }
                };
                let prev = self.prev[prev_slot];
                let s = read(r, &split_set, prev[0], SPLIT_DUP)?;
                let u = read(r, &split_set, prev[1], UPDATE_DUP)?;
                let third_set = self.attr_set(attr, third);
                let t = read(r, &third_set, prev[2], THIRD_DUP)?;
                let mut corners = [0u32; 3];
                corners[split_c] = s;
                corners[new_c] = u;
                corners[third_c] = t;
                match attr {
                    Attr::Diffuse => self.f_dif[fi] = corners,
                    Attr::Specular => self.f_spe[fi] = corners,
                    Attr::Tex(l) => self.f_tex[l][fi] = corners,
                }
                self.prev[prev_slot] = [s, u, t];
            }
        }

        // Adjacency: moving faces leave the split position, new faces join.
        for &f in &move_faces.0 {
            self.adj_mut(split)?.remove(f);
            self.adj_mut(cur)?.add(f);
        }
        for j in 0..new_faces {
            self.link(faces_before + j)?;
        }
        let new_position_faces = self.faces_at(cur);
        // Positions around the new position, taken before the moving faces
        // are rewired (as in the reference).
        let normal_positions = self.positions_of(&new_position_faces);

        // The new position, predicted from the split position.
        let predicted = if cur > 0 {
            self.positions[split as usize]
        } else {
            [0.0; 3]
        };
        let signs = r.compressed_u8(CTX_POSITION_SIGNS);
        let mut pos = [0f32; 3];
        for (k, c) in pos.iter_mut().enumerate() {
            let m = r.compressed_u32(CTX_POSITION_MAG[k]) as f32 * iq_pos;
            *c = if signs & (1 << k) != 0 { -m } else { m } + predicted[k];
        }
        self.positions.push(pos);

        // Rewire moving faces (the resolution increment of the reference).
        for c in &changes {
            let target = match c.attr {
                None => &mut self.f_pos[c.face],
                Some(Attr::Diffuse) => &mut self.f_dif[c.face],
                Some(Attr::Specular) => &mut self.f_spe[c.face],
                Some(Attr::Tex(l)) => &mut self.f_tex[l][c.face],
            };
            target[c.corner] = c.value;
        }

        if !self.exclude_normals {
            self.update_normals(r, &normal_positions, faces_before)?;
        }
        Ok(())
    }

    fn face_normal(&self, face: u32) -> Option<[f32; 3]> {
        let p = self.f_pos[face as usize];
        let get = |i: u32| self.positions.get(i as usize).copied();
        let (a, b, c) = (get(p[0])?, get(p[1])?, get(p[2])?);
        let d1 = sub(b, a);
        let d2 = sub(c, a);
        const EPS: f32 = 1e-6;
        if len(d1) < EPS || len(d2) < EPS {
            return None;
        }
        let n = cross(scale(d1, 1.0 / len(d1)), scale(d2, 1.0 / len(d2)));
        if len(n) < EPS {
            return None;
        }
        Some(scale(n, 1.0 / len(n)))
    }

    /// New normals for every position around the new position.
    fn update_normals(
        &mut self,
        r: &mut BitReader,
        positions: &DescSet,
        faces_before: u32,
    ) -> Result<(), U3dError> {
        let iq = self.inverse_quant[1];
        let old_normals = self.normals.len() as u32;
        let mut new_normals: Vec<[f32; 3]> = Vec::new();
        for &p in &positions.0 {
            let faces = self.faces_at(p);
            let count = r.compressed_u32(CTX_NUM_LOCAL_NORMALS) as usize;
            if count > faces.len().max(1) {
                return Err(damaged("more normals than faces at a position"));
            }
            let face_normals: Vec<[f32; 3]> = faces
                .0
                .iter()
                .filter_map(|&f| self.face_normal(f))
                .collect();

            // Prediction: pick `count` mutually distant face normals, then
            // fold every face normal into the closest one.
            let mut predicted: Vec<[f32; 3]> = Vec::with_capacity(count);
            if let Some(first) = face_normals.first() {
                predicted.push(*first);
            }
            while predicted.len() < count {
                let mut farthest = 1.0f32;
                let mut index = 0;
                for (k, n) in face_normals.iter().enumerate() {
                    let closest = predicted
                        .iter()
                        .map(|q| dot(*n, *q))
                        .fold(-2.0f32, f32::max);
                    if closest < farthest {
                        farthest = closest;
                        index = k;
                    }
                }
                predicted.push(face_normals.get(index).copied().unwrap_or([0.0; 3]));
            }
            let mut contributions = vec![0u32; predicted.len()];
            for n in &face_normals {
                if predicted.is_empty() {
                    break;
                }
                let mut best = 0;
                let mut best_dot = -2.0f32;
                for (l, q) in predicted.iter().enumerate() {
                    let d = dot(*n, *q);
                    if d > best_dot {
                        best_dot = d;
                        best = l;
                    }
                }
                let t = 1.0 / (contributions[best] + 1) as f32;
                predicted[best] = slerp_vectors(t, predicted[best], *n);
                contributions[best] += 1;
            }

            // Each normal is a rotation of its prediction.
            let local: Vec<[f32; 3]> = (0..count)
                .map(|k| {
                    let signs = r.compressed_u8(CTX_NORMAL_SIGNS);
                    let mut d = [0f32; 3];
                    for (i, c) in d.iter_mut().enumerate() {
                        let m = r.compressed_u32(CTX_NORMAL_MAG[i]) as f32 * iq;
                        *c = if signs & (2 << i) != 0 { -m } else { m };
                    }
                    let w2 = (1.0 - dot(d, d)).max(0.0);
                    let w = if signs & 1 != 0 {
                        -w2.sqrt()
                    } else {
                        w2.sqrt()
                    };
                    // <0, predicted> * <w, d>, vector part.
                    let q = predicted[k];
                    let x = cross(q, d);
                    [w * q[0] + x[0], w * q[1] + x[1], w * q[2] + x[2]]
                })
                .collect();

            for &f in &faces.0 {
                let best = r.compressed_u32(CTX_NORMAL_LOCAL_INDEX);
                if best as usize >= count.max(1) {
                    return Err(damaged("normal index out of range"));
                }
                let index = old_normals + new_normals.len() as u32 + best;
                let fi = f as usize;
                if let Some(c) = self.f_pos[fi].iter().position(|x| *x == p) {
                    self.f_nrm[fi][c] = index;
                }
                let _ = faces_before; // new and old faces are updated alike here
            }
            new_normals.extend(local);
        }
        self.normals.extend(new_normals);
        Ok(())
    }

    /// The reconstructed mesh at the highest decoded resolution.
    pub fn finish(self, mesh: &mut Mesh) {
        let layers = self.f_tex.len();
        let mut faces = Vec::with_capacity(self.f_pos.len());
        for i in 0..self.f_pos.len() {
            let (dif, spe, tl) = self.material(i as u32);
            let ok = |idx: &[u32; 3], n: usize| idx.iter().all(|v| (*v as usize) < n);
            let nrm = self.f_nrm[i];
            let d = self.f_dif[i];
            let s = self.f_spe[i];
            faces.push(Face {
                shading: self.f_mat[i],
                position: self.f_pos[i],
                normal: (!self.exclude_normals && ok(&nrm, self.normals.len())).then_some(nrm),
                diffuse: (dif && ok(&d, self.diffuse.len())).then_some(d),
                specular: (spe && ok(&s, self.specular.len())).then_some(s),
                // All layers or none: a face with a broken index in one
                // layer is drawn without textures.
                tex: {
                    let n = (tl as usize).min(layers);
                    if n > 0 && (0..n).all(|l| ok(&self.f_tex[l][i], self.tex.len())) {
                        (0..n).map(|l| self.f_tex[l][i]).collect()
                    } else {
                        Vec::new()
                    }
                },
            });
        }
        mesh.positions = self.positions;
        mesh.normals = self.normals;
        mesh.diffuse = self.diffuse;
        mesh.specular = self.specular;
        mesh.tex_coords = self.tex;
        mesh.faces = faces;
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// Quaternion slerp between the pure quaternions <0,a> and <0,b>
/// (IFXQuaternion::Interpolate), returning the vector part.
fn slerp_vectors(t: f32, a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let mut cos = dot(a, b);
    let mut to = b;
    if cos < 0.0 {
        cos = -cos;
        to = scale(b, -1.0);
    }
    let (s0, s1) = if 1.0 - cos > 1e-6 {
        let omega = cos.min(1.0).acos();
        let sin = omega.sin();
        (((1.0 - t) * omega).sin() / sin, (t * omega).sin() / sin)
    } else {
        (1.0 - t, t)
    };
    [
        s0 * a[0] + s1 * to[0],
        s0 * a[1] + s1 * to[1],
        s0 * a[2] + s1 * to[2],
    ]
}
