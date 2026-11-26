//! 3D annotations: listing, loading a model for the viewer, exporting the
//! raw model data and inserting a new 3D model.

use pdf_types::{DocumentId, RectPt, ThreeDAnnot, ThreeDView};
use serde::Deserialize;

use crate::{snapshot_doc, AppState, DocSnapshot};

/// Every 3D annotation of the document.
#[tauri::command]
pub async fn list_3d(state: tauri::State<'_, AppState>, document: DocumentId) -> Result<Vec<ThreeDAnnot>, String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || -> Result<Vec<ThreeDAnnot>, String> {
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        Ok(mdoc.list_3d())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The model of a 3D annotation, decoded and packed for the viewer
/// (`pdf_3d::render::pack`). Errors starting with `unsupported-format:`
/// name a format the viewer cannot show (e.g. PRC).
#[tauri::command]
pub async fn load_3d_scene(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    page: u32,
    id: String,
) -> Result<tauri::ipc::Response, String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || -> Result<tauri::ipc::Response, String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let (format, model) = mdoc.model_3d(page, &id).map_err(|e| e.to_string())?;
        if format != "U3D" {
            return Err(format!("unsupported-format:{format}"));
        }
        let scene = pdf_3d::u3d::parse(&model).map_err(|e| e.to_string())?;
        let prepared = pdf_3d::render::prepare(&scene);
        let packed = pdf_3d::render::pack(&prepared);
        tracing::info!(
            "load_3d_scene page={page} {}KB model → {} triangles, {} instances, {}KB packed in {:?}; warnings {:?}",
            model.len() / 1024,
            prepared.triangle_count(),
            prepared.instances.len(),
            packed.len() / 1024,
            t.elapsed(),
            scene.warnings
        );
        Ok(tauri::ipc::Response::new(packed))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Writes the raw model data (U3D/PRC as embedded) of a 3D annotation.
#[tauri::command]
pub async fn export_3d_model(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    page: u32,
    id: String,
    path: String,
) -> Result<(), String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let (_, model) = mdoc.model_3d(page, &id).map_err(|e| e.to_string())?;
        std::fs::write(&path, model).map_err(|e| format!("write {path}: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// File types the 3D inserter may read: model files and what they
/// reference (glTF buffers and images, OBJ material libraries).
const MODEL_FILE_TYPES: &[&str] = &["glb", "gltf", "bin", "obj", "mtl", "stl", "ply", "3mf", "png", "jpg", "jpeg", "webp", "ktx2"];

/// Reads a 3D model file (or a file it references) for the inserter.
#[tauri::command]
pub async fn read_3d_file(path: String) -> Result<tauri::ipc::Response, String> {
    let ext = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if !MODEL_FILE_TYPES.contains(&ext.as_str()) {
        return Err(format!("not a 3D model file: {path}"));
    }
    let meta = tokio::fs::metadata(&path).await.map_err(|e| format!("{path}: {e}"))?;
    if meta.len() > MAX_UPLOAD as u64 {
        return Err(format!("{path}: file too large"));
    }
    let bytes = tokio::fs::read(&path).await.map_err(|e| format!("{path}: {e}"))?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[derive(Deserialize, Clone, Copy)]
struct Range {
    offset: u32,
    length: u32,
}

#[derive(Deserialize)]
struct InsertMesh {
    name: String,
    color: [f32; 4],
    matrix: [f32; 16],
    positions: Range,
    #[serde(default)]
    normals: Option<Range>,
    indices: Range,
}

#[derive(Deserialize)]
struct InsertHeader {
    document: DocumentId,
    page: u32,
    rect: RectPt,
    name: String,
    activation: String,
    views: Vec<ThreeDView>,
    default_view: usize,
    units_m: Option<f64>,
    poster: Range,
    meshes: Vec<InsertMesh>,
}

/// Upper bound for one upload (models + poster).
const MAX_UPLOAD: usize = 1 << 30;

fn slice(data: &[u8], r: Range) -> Result<&[u8], String> {
    let (o, l) = (r.offset as usize, r.length as usize);
    data.get(o..o.checked_add(l).ok_or("range overflow")?).ok_or_else(|| "range outside upload".to_string())
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// Parses the upload of `insert_3d_model`: `"D3I1"`, u32 header length,
/// JSON header, padding to 4 bytes, data section (ranges are relative to
/// it). Returns the header and the U3D file built from the meshes.
fn parse_upload(body: &[u8]) -> Result<(InsertHeader, Vec<u8>, Vec<u8>), String> {
    if body.len() > MAX_UPLOAD || body.len() < 8 || &body[..4] != b"D3I1" {
        return Err("3D upload: bad format".into());
    }
    let n = u32::from_le_bytes([body[4], body[5], body[6], body[7]]) as usize;
    let json = body.get(8..8 + n).ok_or("3D upload: header")?;
    let header: InsertHeader = serde_json::from_slice(json).map_err(|e| format!("3D upload header: {e}"))?;
    let data = &body[(8 + n).div_ceil(4) * 4..];

    let mut meshes = Vec::new();
    for m in &header.meshes {
        let p = f32s(slice(data, m.positions)?);
        let positions: Vec<[f32; 3]> = p.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
        let normals: Vec<[f32; 3]> = match m.normals {
            Some(r) => f32s(slice(data, r)?).chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
            None => Vec::new(),
        };
        let idx: Vec<u32> = slice(data, m.indices)?
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        if idx.iter().any(|i| *i as usize >= positions.len()) {
            return Err(format!("3D upload: mesh {} has an index out of range", m.name));
        }
        let triangles: Vec<[u32; 3]> = idx.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
        if triangles.is_empty() {
            continue;
        }
        meshes.push(pdf_3d::u3d::ExportMesh {
            name: m.name.clone(),
            parent: None,
            normals: if normals.len() == positions.len() { normals } else { Vec::new() },
            positions,
            triangles,
            color: m.color,
            transform: m.matrix,
        });
    }
    if meshes.is_empty() {
        return Err("3D upload: the model has no triangles".into());
    }
    let u3d = pdf_3d::u3d::write(&pdf_3d::u3d::ExportScene { groups: Vec::new(), meshes, units_m: header.units_m });
    let poster = slice(data, header.poster)?.to_vec();
    Ok((header, u3d, poster))
}

/// Inserts a 3D model as a 3D annotation. The body is binary (see
/// `parse_upload`): the UI loads the model file with three.js, sends the
/// triangle meshes, the preset views and a rendered poster.
#[tauri::command]
pub async fn insert_3d_model(
    state: tauri::State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> Result<DocSnapshot, String> {
    let tauri::ipc::InvokeBody::Raw(body) = request.body() else {
        return Err("insert_3d_model: expected a binary body".into());
    };
    let (header, u3d, poster) = parse_upload(body)?;
    let document = header.document;
    let engine = state.engine.clone();
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let u3d_len = u3d.len();
        mdoc.add_3d_annotation(
            header.page,
            &pdf_mutate::ThreeDAnnotation {
                rect: header.rect,
                u3d,
                views: header.views,
                default_view: header.default_view,
                poster,
                name: header.name,
                activation: header.activation,
            },
        )
        .map_err(|e| e.to_string())?;
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!("insert_3d_model page={} u3d={}KB: {:?}", header.page, u3d_len / 1024, t.elapsed());
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    snapshot_doc(state, document).await
}
