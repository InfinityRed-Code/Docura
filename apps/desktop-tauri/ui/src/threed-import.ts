// "Insert 3D model": load a model file with three.js, let the user check it
// in a preview (orientation, units), then place it on the page as a 3D
// annotation. The backend writes the meshes as U3D (pdf_3d::u3d::write).
//
// What goes into the PDF is geometry and one color per part; textures and
// per-vertex colors are not exported, so the preview shows exactly that.

import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { OBJLoader } from "three/examples/jsm/loaders/OBJLoader.js";
import { MTLLoader } from "three/examples/jsm/loaders/MTLLoader.js";
import { STLLoader } from "three/examples/jsm/loaders/STLLoader.js";
import { PLYLoader } from "three/examples/jsm/loaders/PLYLoader.js";
import { ThreeMFLoader } from "three/examples/jsm/loaders/3MFLoader.js";
import { mergeVertices, toCreasedNormals } from "three/examples/jsm/utils/BufferGeometryUtils.js";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { state } from "./state";
import { setStatus } from "./dom";
import { t } from "./i18n";
import { refreshIcons } from "./icons";
import { applyDocSnapshot } from "./doc";
import { forget3dCaches } from "./threed";
import { renderPoster, srgb, type Model3D } from "./threed-view";
import { standardViews, type Bounds, type Vec3 } from "./threed-camera";
import type { DocSnapshot, RectPt } from "./types";

export const MODEL_EXTENSIONS = ["glb", "gltf", "obj", "stl", "ply", "3mf"];

/** One part as it will be written: model-space triangles and a color. */
type Part = {
  name: string;
  geometry: THREE.BufferGeometry; // indexed, position + normal
  matrix: THREE.Matrix4; // part to model space (before the up-axis turn)
  color: [number, number, number, number];
};

type Loaded = { fileName: string; ext: string; parts: Part[]; triangles: number };

type Pending = Loaded & { zUp: boolean; unitsM: number | null };
let pending: Pending | null = null;

async function readFile(path: string): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("read_3d_file", { path });
}

function dirOf(path: string) {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i >= 0 ? path.slice(0, i + 1) : "";
}

function baseName(path: string) {
  return path.replace(/\\/g, "/").split("/").pop() ?? path;
}

/** Resolves a relative URI of a model file against its directory. */
function sibling(dir: string, uri: string) {
  return dir + decodeURIComponent(uri).replace(/^\.\//, "");
}

async function loadObject(path: string): Promise<THREE.Object3D> {
  const ext = path.split(".").pop()!.toLowerCase();
  const dir = dirOf(path);
  const buf = await readFile(path);
  switch (ext) {
    case "glb":
    case "gltf": {
      // External buffers/images are read up front and served as blob URLs.
      const urls = new Map<string, string>();
      if (ext === "gltf") {
        const json = JSON.parse(new TextDecoder().decode(buf));
        const refs: string[] = [
          ...(json.buffers ?? []).map((b: { uri?: string }) => b.uri),
          ...(json.images ?? []).map((i: { uri?: string }) => i.uri),
        ].filter((u: string | undefined): u is string => !!u && !u.startsWith("data:"));
        for (const r of refs) {
          try {
            urls.set(r, URL.createObjectURL(new Blob([await readFile(sibling(dir, r))])));
          } catch {
            // Missing textures are fine; missing buffers fail in the parser.
          }
        }
      }
      const manager = new THREE.LoadingManager();
      manager.setURLModifier((u) => {
        for (const [r, blob] of urls) if (u.endsWith(r) || decodeURIComponent(u).endsWith(r)) return blob;
        return u;
      });
      const gltf = await new GLTFLoader(manager).parseAsync(buf, "");
      urls.forEach((u) => URL.revokeObjectURL(u));
      return gltf.scene;
    }
    case "obj": {
      const text = new TextDecoder().decode(buf);
      const loader = new OBJLoader();
      const lib = /^mtllib\s+(.+)$/m.exec(text)?.[1]?.trim();
      if (lib) {
        try {
          const mtl = new TextDecoder().decode(await readFile(sibling(dir, lib)));
          const materials = new MTLLoader().parse(mtl, "");
          materials.preload();
          loader.setMaterials(materials);
        } catch {
          // Without the material library the parts get the default color.
        }
      }
      return loader.parse(text);
    }
    case "stl": {
      const geo = new STLLoader().parse(buf);
      const color = (geo as THREE.BufferGeometry & { hasColors?: boolean }).hasColors ? 0xffffff : 0xb8bec7;
      return new THREE.Mesh(geo, new THREE.MeshPhongMaterial({ color }));
    }
    case "ply": {
      const geo = new PLYLoader().parse(buf);
      return new THREE.Mesh(geo, new THREE.MeshPhongMaterial({ color: 0xb8bec7 }));
    }
    case "3mf":
      return new ThreeMFLoader().parse(buf);
  }
  throw new Error(`unsupported file type .${ext}`);
}

/** Indexed geometry with normals that keep hard edges (CAD exports often
 * come as triangle soup without usable normals). */
function prepareGeometry(src: THREE.BufferGeometry, start: number, count: number): THREE.BufferGeometry {
  let g = new THREE.BufferGeometry();
  const pos = src.getAttribute("position");
  const idx = src.getIndex();
  const n = Math.min(count, (idx ? idx.count : pos.count) - start);
  const out = new Float32Array(n * 3);
  for (let i = 0; i < n; i++) {
    const v = idx ? idx.getX(start + i) : start + i;
    out[i * 3] = pos.getX(v);
    out[i * 3 + 1] = pos.getY(v);
    out[i * 3 + 2] = pos.getZ(v);
  }
  g.setAttribute("position", new THREE.BufferAttribute(out, 3));
  g = toCreasedNormals(g, Math.PI / 6);
  g.deleteAttribute("uv");
  g = mergeVertices(g, 1e-6);
  return g;
}

function materialColor(m: THREE.Material | undefined): [number, number, number, number] {
  const c = (m as THREE.MeshStandardMaterial | undefined)?.color;
  // Material colors are in three.js' linear working space; PDF wants sRGB.
  const rgb = c ? c.clone().convertLinearToSRGB() : { r: 0.72, g: 0.75, b: 0.78 };
  const opacity = m && m.transparent ? m.opacity : 1;
  return [rgb.r, rgb.g, rgb.b, Math.max(0.05, Math.min(1, opacity))];
}

function collectParts(root: THREE.Object3D): Part[] {
  root.updateMatrixWorld(true);
  const parts: Part[] = [];
  root.traverse((o) => {
    const mesh = o as THREE.Mesh;
    if (!mesh.isMesh || !mesh.geometry?.getAttribute("position")) return;
    const geo = mesh.geometry as THREE.BufferGeometry;
    const mats = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
    const total = geo.getIndex()?.count ?? geo.getAttribute("position").count;
    const groups = geo.groups.length > 0 ? geo.groups : [{ start: 0, count: total, materialIndex: 0 }];
    const name = mesh.name || mesh.parent?.name || "";
    for (const grp of groups) {
      if (grp.count < 3) continue;
      parts.push({
        name: groups.length > 1 ? `${name} ${(grp.materialIndex ?? 0) + 1}`.trim() : name,
        geometry: prepareGeometry(geo, grp.start, grp.count),
        matrix: mesh.matrixWorld.clone(),
        color: materialColor(mats[grp.materialIndex ?? 0] ?? mats[0]),
      });
    }
  });
  return parts;
}

const Y_TO_Z = new THREE.Matrix4().makeRotationX(Math.PI / 2);

function partMatrix(p: Part, zUp: boolean): THREE.Matrix4 {
  return zUp ? p.matrix.clone() : Y_TO_Z.clone().multiply(p.matrix);
}

/** A Model3D (as the viewer uses) of what will be written. */
function previewModel(l: Loaded, zUp: boolean): Model3D {
  const root = new THREE.Group();
  const meshes: THREE.Mesh[] = [];
  const box = new THREE.Box3();
  const mats: THREE.Material[] = [];
  for (const p of l.parts) {
    const mat = new THREE.MeshPhongMaterial({
      color: srgb(p.color[0], p.color[1], p.color[2]),
      opacity: p.color[3],
      transparent: p.color[3] < 1,
      side: THREE.DoubleSide,
      specular: srgb(0.075, 0.075, 0.075),
      shininess: 20,
    });
    mats.push(mat);
    const mesh = new THREE.Mesh(p.geometry, mat);
    mesh.matrixAutoUpdate = false;
    mesh.matrix.copy(partMatrix(p, zUp));
    mesh.userData = { path: [p.name], node: p.name, geometry: meshes.length, material: mat };
    root.add(mesh);
    meshes.push(mesh);
    p.geometry.computeBoundingBox();
    box.union(p.geometry.boundingBox!.clone().applyMatrix4(mesh.matrix));
  }
  root.updateMatrixWorld(true);
  const bounds: Bounds | null = box.isEmpty() ? null : { min: box.min.toArray() as Vec3, max: box.max.toArray() as Vec3 };
  return {
    root,
    meshes,
    bounds,
    header: {
      geometries: [], instances: [], materials: [], textures: [], tree: [], cameras: [], lights: [],
      bounds: bounds ? [bounds.min, bounds.max] : null, units_m: null, warnings: [], triangles: l.triangles,
    },
    dispose: () => mats.forEach((m) => m.dispose()),
  };
}

const DEFAULT_UNITS: Record<string, number | null> = { glb: 1, gltf: 1, "3mf": 0.001, stl: 0.001, obj: null, ply: null };
const DEFAULT_Y_UP = new Set(["glb", "gltf", "obj"]);

/** Preview dialog: orientation and units. Resolves true to place it. */
function previewDialog(l: Loaded): Promise<Pending | null> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "propsOverlay threeDImport";
    overlay.innerHTML = `
      <div class="propsModal" role="dialog" aria-modal="true">
        <div class="propsHeader"><strong>${t("3d.insertTitle")}</strong></div>
        <div class="propsPanel wmPanel">
          <div class="threeDImportView"></div>
          <p class="hfHint threeDImportInfo"></p>
          <div class="threeDImportOpts">
            <label>${t("3d.up")} <select data-k="up"><option value="z">Z</option><option value="y">Y</option></select></label>
            <label>${t("3d.units")} <select data-k="units">
              <option value="0.001">mm</option><option value="0.01">cm</option><option value="1">m</option>
              <option value="0.0254">in</option><option value="">${t("3d.unitsNone")}</option>
            </select></label>
          </div>
          <p class="hfHint">${t("3d.insertNote")}</p>
          <div class="wmActions">
            <button type="button" data-k="cancel">${t("tip.cancel")}</button>
            <button type="button" data-k="ok" class="primaryBtn">${t("3d.place")}</button>
          </div>
        </div>
      </div>`;
    document.body.appendChild(overlay);
    const host = overlay.querySelector<HTMLDivElement>(".threeDImportView")!;
    const info = overlay.querySelector<HTMLParagraphElement>(".threeDImportInfo")!;
    const upSel = overlay.querySelector<HTMLSelectElement>('[data-k="up"]')!;
    const unitSel = overlay.querySelector<HTMLSelectElement>('[data-k="units"]')!;
    upSel.value = DEFAULT_Y_UP.has(l.ext) ? "y" : "z";
    const u = DEFAULT_UNITS[l.ext];
    unitSel.value = u === null || u === undefined ? "" : String(u);
    info.textContent = t("3d.insertInfo", { file: l.fileName, parts: l.parts.length, n: l.triangles.toLocaleString() });

    const w = 520, h = 320;
    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(Math.min(2, window.devicePixelRatio || 1));
    renderer.setSize(w, h);
    host.appendChild(renderer.domElement);
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0xffffff);
    const cam = new THREE.PerspectiveCamera(30, w / h);
    scene.add(cam);
    const headlamp = new THREE.DirectionalLight(0xffffff, 2.2);
    headlamp.position.set(0.3, 0.5, 1);
    cam.add(headlamp);
    scene.add(new THREE.AmbientLight(0xffffff, 0.9));
    const controls = new OrbitControls(cam, renderer.domElement);
    let model: Model3D | null = null;
    const draw = () => renderer.render(scene, cam);
    controls.addEventListener("change", draw);
    const rebuild = () => {
      if (model) {
        scene.remove(model.root);
        model.dispose();
      }
      model = previewModel(l, upSel.value === "z");
      scene.add(model.root);
      const b = model.bounds;
      if (b) {
        const c = new THREE.Vector3(...b.min).add(new THREE.Vector3(...b.max)).multiplyScalar(0.5);
        const r = new THREE.Vector3(...b.max).distanceTo(new THREE.Vector3(...b.min)) / 2 || 1;
        const d = r / Math.sin((15 * Math.PI) / 180);
        cam.up.set(0, 0, 1);
        cam.position.copy(c).add(new THREE.Vector3(1, -1, 0.8).normalize().multiplyScalar(d));
        cam.near = d / 100;
        cam.far = d * 10;
        cam.updateProjectionMatrix();
        controls.target.copy(c);
        controls.update();
      }
      draw();
    };
    rebuild();
    upSel.addEventListener("change", rebuild);
    const finish = (ok: boolean) => {
      controls.dispose();
      if (model) {
        scene.remove(model.root);
        model.dispose();
      }
      renderer.dispose();
      renderer.forceContextLoss();
      overlay.remove();
      resolve(ok ? { ...l, zUp: upSel.value === "z", unitsM: unitSel.value ? parseFloat(unitSel.value) : null } : null);
    };
    overlay.querySelector('[data-k="cancel"]')!.addEventListener("click", () => finish(false));
    overlay.querySelector('[data-k="ok"]')!.addEventListener("click", () => finish(true));
    overlay.addEventListener("keydown", (e) => {
      if (e.key === "Escape") finish(false);
    });
    refreshIcons();
  });
}

/** Picks and loads a model file; true when the user wants to place it. */
export async function pickModel(given?: string): Promise<boolean> {
  const path = given ?? await open({
    multiple: false,
    directory: false,
    filters: [{ name: t("3d.modelFiles"), extensions: MODEL_EXTENSIONS }],
    title: t("3d.pick"),
  });
  if (!path || typeof path !== "string") return false;
  setStatus(t("3d.reading", { file: baseName(path) }));
  let loaded: Loaded;
  try {
    const root = await loadObject(path);
    const parts = collectParts(root);
    const triangles = parts.reduce((n, p) => n + (p.geometry.getIndex()?.count ?? 0) / 3, 0);
    if (parts.length === 0 || triangles === 0) throw new Error(t("3d.noTriangles"));
    loaded = { fileName: baseName(path), ext: path.split(".").pop()!.toLowerCase(), parts, triangles };
  } catch (e) {
    setStatus(t("3d.readError", { e: String(e) }));
    return false;
  }
  setStatus("");
  pending = await previewDialog(loaded);
  if (!pending) return false;
  setStatus(t("3d.placeHint"));
  return true;
}

/** Builds the D3I1 upload (see src-tauri/src/threed.rs). */
function packUpload(header: Record<string, unknown>, blobs: Uint8Array[]): Uint8Array {
  const json = new TextEncoder().encode(JSON.stringify(header));
  const head = 8 + json.length;
  const dataStart = Math.ceil(head / 4) * 4;
  let size = 0;
  for (const b of blobs) size = Math.ceil(size / 4) * 4 + b.length;
  const out = new Uint8Array(dataStart + size);
  out.set([0x44, 0x33, 0x49, 0x31], 0); // "D3I1"
  new DataView(out.buffer).setUint32(4, json.length, true);
  out.set(json, 8);
  out.fill(0x20, head, dataStart);
  let off = 0;
  for (const b of blobs) {
    off = Math.ceil(off / 4) * 4;
    out.set(b, dataStart + off);
    off += b.length;
  }
  return out;
}

/** Inserts the picked model into `rect` on `page`. */
export async function insertAt(page: number, rect: RectPt) {
  const p = pending;
  if (!p || !state.currentDoc) return;
  // Keep the model on the page.
  const sz = state.pageSizes[page];
  if (sz) {
    rect = {
      x0: Math.max(0, rect.x0),
      y0: Math.max(0, rect.y0),
      x1: Math.min(sz.width_pt, rect.x1),
      y1: Math.min(sz.height_pt, rect.y1),
    };
  }
  const wPt = rect.x1 - rect.x0;
  const hPt = rect.y1 - rect.y0;
  if (wPt < 20 || hPt < 20) {
    setStatus(t("3d.tooSmall"));
    return;
  }
  setStatus(t("3d.inserting"));
  const model = previewModel(p, p.zUp);
  try {
    const names: Record<string, string> = {};
    for (const k of ["iso", "front", "back", "left", "right", "top", "bottom"]) names[k] = t(`3d.std.${k}`);
    const views = standardViews(model.bounds ?? { min: [-1, -1, -1], max: [1, 1, 1] }, names, wPt, hPt);
    // Poster: the default (iso) view at twice the box resolution.
    const s = Math.min(3, 2400 / Math.max(wPt, hPt));
    const dataUrl = renderPoster(model, views[0], Math.round(wPt * s), Math.round(hPt * s), wPt, hPt);
    const poster = Uint8Array.from(atob(dataUrl.split(",")[1]), (c) => c.charCodeAt(0));

    const blobs: Uint8Array[] = [];
    const range = (b: Uint8Array) => {
      let off = 0;
      for (const x of blobs) off = Math.ceil(off / 4) * 4 + x.length;
      off = Math.ceil(off / 4) * 4;
      blobs.push(b);
      return { offset: off, length: b.length };
    };
    const asBytes = (a: ArrayLike<number> & { buffer: ArrayBufferLike; byteOffset: number; byteLength: number }) =>
      new Uint8Array(a.buffer, a.byteOffset, a.byteLength);
    const meshes = p.parts.map((part) => {
      const g = part.geometry;
      const pos = g.getAttribute("position").array as Float32Array;
      const nrm = g.getAttribute("normal")?.array as Float32Array | undefined;
      const idx = g.getIndex()!;
      const indices = idx.array instanceof Uint32Array ? idx.array : Uint32Array.from(idx.array as ArrayLike<number>);
      return {
        name: part.name,
        color: part.color,
        matrix: partMatrix(part, p.zUp).toArray(),
        positions: range(asBytes(Float32Array.from(pos))),
        normals: nrm ? range(asBytes(Float32Array.from(nrm))) : null,
        indices: range(asBytes(indices)),
      };
    });
    const header = {
      document: state.currentDoc,
      page,
      rect,
      name: p.fileName,
      activation: "XA",
      views,
      default_view: 0,
      units_m: p.unitsM,
      poster: range(poster),
      meshes,
    };
    const body = packUpload(header, blobs);
    const snap = await invoke<DocSnapshot>("insert_3d_model", body);
    forget3dCaches();
    applyDocSnapshot(snap);
    pending = null;
    setStatus(t("3d.inserted", { file: p.fileName }));
  } catch (e) {
    setStatus(t("3d.insertError", { e: String(e) }));
  } finally {
    model.dispose();
  }
}
