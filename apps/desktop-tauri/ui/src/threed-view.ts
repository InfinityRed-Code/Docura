// Interactive 3D view of a PDF 3D annotation (three.js / WebGL). Loaded on
// demand: nothing here runs for documents without 3D content.
//
// The backend decodes the model (U3D) and sends one packed buffer
// (pdf_3d::render::pack): indexed geometries in model space, instances
// placing them, materials, textures, the node tree, U3D cameras and lights.

import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { t } from "./i18n";
import type { ThreeDAnnot, ThreeDView } from "./types";
import {
  cameraForView, fitCamera, LIGHTING, LIGHTING_NAMES, RENDER_MODES,
  type Bounds, type CameraSpec, type Vec3,
} from "./threed-camera";

/** PDF/U3D colors are sRGB values; three.js works in linear sRGB. */
export const srgb = (r: number, g: number, b: number) => new THREE.Color().setRGB(r, g, b, THREE.SRGBColorSpace);

type Range = { offset: number; length: number };
type PackedHeader = {
  geometries: { positions: Range; normals: Range | null; colors: Range | null; uvs: Range | null; indices: Range }[];
  instances: { node: string; path: string[]; matrix: number[]; geometry: number; material: number }[];
  materials: {
    diffuse: [number, number, number, number];
    ambient: Vec3;
    specular: Vec3;
    emissive: Vec3;
    texture: number | null;
    vertex_colors: boolean;
    lighting: boolean;
  }[];
  textures: { name: string; mime: string; data: Range }[];
  tree: { name: string; kind: string; parents: string[] }[];
  cameras: { name: string; matrix: number[]; orthographic: boolean; projection: number }[];
  lights: { light_type: number; color: Vec3; intensity: number; spot_angle: number; matrix: number[] }[];
  bounds: [Vec3, Vec3] | null;
  units_m: number | null;
  warnings: string[];
  triangles: number;
};

/** A decoded scene ready to be added to a three.js scene. */
export type Model3D = {
  root: THREE.Group;
  meshes: THREE.Mesh[];
  header: PackedHeader;
  bounds: Bounds | null;
  dispose: () => void;
};

export function unpackScene(buf: ArrayBuffer): Model3D {
  const bytes = new Uint8Array(buf);
  const magic = String.fromCharCode(...bytes.subarray(0, 4));
  if (magic !== "D3S1") throw new Error("bad 3D scene data");
  const n = new DataView(buf).getUint32(4, true);
  const header = JSON.parse(new TextDecoder().decode(bytes.subarray(8, 8 + n))) as PackedHeader;
  const base = Math.ceil((8 + n) / 4) * 4;
  const f32 = (r: Range) => new Float32Array(buf, base + r.offset, r.length / 4);
  const u32 = (r: Range) => new Uint32Array(buf, base + r.offset, r.length / 4);

  const urls: string[] = [];
  const textures = header.textures.map((tx) => {
    const blob = new Blob([bytes.subarray(base + tx.data.offset, base + tx.data.offset + tx.data.length)], { type: tx.mime });
    const url = URL.createObjectURL(blob);
    urls.push(url);
    const tex = new THREE.TextureLoader().load(url);
    tex.colorSpace = THREE.SRGBColorSpace;
    tex.wrapS = tex.wrapT = THREE.RepeatWrapping;
    return tex;
  });
  const geometries = header.geometries.map((g) => {
    let geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.BufferAttribute(f32(g.positions), 3));
    if (g.normals) geo.setAttribute("normal", new THREE.BufferAttribute(f32(g.normals), 3));
    if (g.colors) geo.setAttribute("color", new THREE.BufferAttribute(f32(g.colors), 3));
    if (g.uvs) geo.setAttribute("uv", new THREE.BufferAttribute(f32(g.uvs), 2));
    geo.setIndex(new THREE.BufferAttribute(u32(g.indices), 1));
    if (!g.normals) {
      // No normals in the file: flat shading.
      geo = geo.toNonIndexed();
      geo.computeVertexNormals();
    }
    geo.computeBoundingBox();
    geo.computeBoundingSphere();
    return geo;
  });
  const materials = header.materials.map((m) => {
    const opts = {
      color: srgb(m.diffuse[0], m.diffuse[1], m.diffuse[2]),
      opacity: m.diffuse[3],
      transparent: m.diffuse[3] < 0.999,
      side: THREE.DoubleSide,
      vertexColors: m.vertex_colors,
      map: m.texture !== null ? textures[m.texture] ?? null : null,
    };
    const mat = m.lighting
      ? new THREE.MeshPhongMaterial({
          ...opts,
          specular: srgb(m.specular[0] * 0.3, m.specular[1] * 0.3, m.specular[2] * 0.3),
          emissive: srgb(...m.emissive),
          shininess: 20,
        })
      : new THREE.MeshBasicMaterial(opts);
    if (mat.map) mat.color.setRGB(1, 1, 1); // textures replace the diffuse color
    return mat;
  });
  const root = new THREE.Group();
  const meshes: THREE.Mesh[] = [];
  for (const inst of header.instances) {
    const mesh = new THREE.Mesh(geometries[inst.geometry], materials[inst.material]);
    mesh.matrixAutoUpdate = false;
    mesh.matrix.fromArray(inst.matrix);
    mesh.userData = { path: inst.path, node: inst.node, geometry: inst.geometry, material: mesh.material };
    root.add(mesh);
    meshes.push(mesh);
  }
  root.updateMatrixWorld(true);
  const bounds: Bounds | null = header.bounds ? { min: header.bounds[0], max: header.bounds[1] } : null;
  return {
    root,
    meshes,
    header,
    bounds,
    dispose: () => {
      geometries.forEach((g) => g.dispose());
      materials.forEach((m) => m.dispose());
      textures.forEach((tx) => tx.dispose());
      urls.forEach((u) => URL.revokeObjectURL(u));
    },
  };
}

const EMPTY_BOUNDS: Bounds = { min: [-1, -1, -1], max: [1, 1, 1] };

const BLANK_VIEW: ThreeDView = {
  name: "", internal_name: null, c2w: null, u3d_path: [], center_of_orbit: null,
  orthographic: false, fov: null, fov_span: null, ortho_scale: null, ortho_binding: null,
  background: null, render_mode: null, render_aux_color: null, lighting: null,
  cross_sections: [], nodes: [],
};

/** Camera for `view`; without one, the model's own camera (first U3D view
 * node), else an isometric fit (Z up). */
function resolveCamera(model: Model3D, view: ThreeDView | null, wPt: number, hPt: number): CameraSpec {
  const cams = model.header.cameras;
  if (view) {
    const c = cameraForView(view, model.bounds, wPt, hPt, cams);
    if (c) return c;
  }
  const own = cams[0];
  if (own && !own.orthographic) {
    const fov = own.projection > 1 && own.projection < 179 ? own.projection : 34;
    const c = cameraForView({ ...BLANK_VIEW, u3d_path: [own.name], fov, fov_span: "H" }, model.bounds, wPt, hPt, cams);
    if (c) return c;
  }
  return fitCamera(model.bounds ?? EMPTY_BOUNDS, [1, -1, 0.8], [0, 0, 1], 30, wPt / hPt);
}

function applyCamera(
  spec: CameraSpec,
  persp: THREE.PerspectiveCamera,
  ortho: THREE.OrthographicCamera,
  aspect: number,
  bounds: Bounds | null,
): THREE.Camera {
  const r = bounds ? Math.hypot(...bounds.max.map((v, i) => v - bounds.min[i])) / 2 : 1;
  const dist = Math.hypot(...spec.position.map((v, i) => v - spec.target[i]));
  const cam = spec.orthographic ? ortho : persp;
  cam.position.set(...spec.position);
  cam.up.set(...spec.up);
  cam.lookAt(...spec.target);
  if (spec.orthographic) {
    const hh = spec.orthoHalfHeight > 0 ? spec.orthoHalfHeight : r;
    ortho.top = hh;
    ortho.bottom = -hh;
    ortho.left = -hh * aspect;
    ortho.right = hh * aspect;
    ortho.near = -(dist + r) * 4;
    ortho.far = (dist + r) * 4;
    ortho.updateProjectionMatrix();
  } else {
    persp.fov = spec.fovV;
    persp.aspect = aspect;
    persp.near = Math.max(dist - r * 1.5, dist / 1000, r / 1e4, 1e-6);
    persp.far = (dist + r) * 4;
    persp.updateProjectionMatrix();
  }
  return cam;
}

/** Lights for a lighting scheme, attached to the camera (scheme lights are
 * defined in camera space). "Artwork" uses the model's own lights. */
function buildLights(scheme: string, model: Model3D, camera: THREE.Camera, scene: THREE.Scene): THREE.Object3D[] {
  const made: THREE.Object3D[] = [];
  const useArtwork = (scheme === "Artwork" || !LIGHTING[scheme]) && scheme !== "None";
  if (useArtwork && model.header.lights.length > 0) {
    for (const l of model.header.lights) {
      const color = srgb(...l.color);
      const intensity = Math.max(0, l.intensity || 1) * Math.PI * 0.8;
      const m = new THREE.Matrix4().fromArray(l.matrix);
      const pos = new THREE.Vector3().setFromMatrixPosition(m);
      const dir = new THREE.Vector3(0, 0, -1).transformDirection(m);
      let light: THREE.Light;
      if (l.light_type === 0) light = new THREE.AmbientLight(color, intensity * 0.5);
      else if (l.light_type === 1) {
        const d = new THREE.DirectionalLight(color, intensity);
        d.position.copy(dir.clone().multiplyScalar(-1));
        light = d;
      } else if (l.light_type === 3) {
        const s = new THREE.SpotLight(color, intensity, 0, ((l.spot_angle || 45) * Math.PI) / 180, 0.2, 0);
        s.position.copy(pos);
        s.target.position.copy(pos.clone().add(dir));
        scene.add(s.target);
        made.push(s.target);
        light = s;
      } else {
        const p = new THREE.PointLight(color, intensity, 0, 0);
        p.position.copy(pos);
        light = p;
      }
      scene.add(light);
      made.push(light);
    }
    // Files often light only one side; a little ambient keeps the rest visible.
    const amb = new THREE.AmbientLight(0xffffff, 0.35);
    scene.add(amb);
    made.push(amb);
    return made;
  }
  const def = LIGHTING[scheme === "None" ? "None" : LIGHTING[scheme] ? scheme : "CAD"];
  if (!def) {
    const amb = new THREE.AmbientLight(0xffffff, 0.6);
    scene.add(amb);
    return [amb];
  }
  const amb = new THREE.AmbientLight(srgb(...def.ambient), Math.PI * 0.6);
  scene.add(amb);
  made.push(amb);
  for (const l of def.lights) {
    const light = new THREE.DirectionalLight(srgb(...l.color), Math.PI * 0.9);
    const d = new THREE.Vector3(l.dir[0], l.dir[1], -l.dir[2]);
    if (d.lengthSq() === 0) d.set(0, 0, -1); // along the line of sight
    // Light travels along d (camera space): place it opposite.
    light.position.copy(d.clone().normalize().multiplyScalar(-1));
    light.target.position.set(0, 0, 0);
    camera.add(light);
    camera.add(light.target);
    made.push(light, light.target);
  }
  return made;
}

/** One shared offscreen renderer for posters and previews (WebGL contexts
 * are limited). */
let posterRenderer: THREE.WebGLRenderer | null = null;

/** Renders `model` in `view` to a PNG data URL of `wPx` x `hPx`. */
export function renderPoster(
  model: Model3D,
  view: ThreeDView | null,
  wPx: number,
  hPx: number,
  wPt: number,
  hPt: number,
): string {
  if (!posterRenderer) {
    posterRenderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true, alpha: false });
  }
  const r = posterRenderer;
  r.setPixelRatio(1);
  r.setSize(wPx, hPx, false);
  const scene = new THREE.Scene();
  const bg = view?.background ?? [1, 1, 1];
  scene.background = srgb(bg[0], bg[1], bg[2]);
  const persp = new THREE.PerspectiveCamera();
  const ortho = new THREE.OrthographicCamera();
  const cam = applyCamera(resolveCamera(model, view, wPt, hPt), persp, ortho, wPx / hPx, model.bounds);
  scene.add(cam);
  scene.add(model.root);
  const lights = buildLights(view?.lighting ?? "Artwork", model, cam, scene);
  r.render(scene, cam);
  const url = r.domElement.toDataURL("image/png");
  scene.remove(model.root);
  lights.forEach((l) => l.removeFromParent());
  return url;
}

// ---------- Render modes ----------

type ModeParts = { solid: boolean; transparent: boolean; wire: boolean; edges: boolean; hiddenLine: boolean; flat: boolean; points: boolean; box: boolean };

function modeParts(mode: string): ModeParts {
  const p: ModeParts = { solid: true, transparent: false, wire: false, edges: false, hiddenLine: false, flat: false, points: false, box: false };
  switch (mode) {
    case "SolidWireframe": p.wire = true; break;
    case "Transparent": p.transparent = true; break;
    case "TransparentWireframe": p.transparent = true; p.wire = true; break;
    case "Wireframe": case "ShadedWireframe": p.solid = false; p.wire = true; break;
    case "HiddenWireframe": p.solid = false; p.wire = true; p.hiddenLine = true; break;
    case "Illustration": p.flat = true; p.edges = true; break;
    case "SolidOutline": case "ShadedIllustration": p.edges = true; break;
    case "Vertices": case "ShadedVertices": p.solid = false; p.points = true; break;
    case "BoundingBox": p.solid = false; p.box = true; break;
    case "TransparentBoundingBox": case "TransparentBoundingBoxOutline": p.transparent = true; p.box = true; break;
  }
  return p;
}

// ---------- The interactive viewer ----------

export type ViewerOptions = {
  annot: ThreeDAnnot;
  model: Model3D;
  /** Box size in points (for the PDF projection rules). */
  wPt: number;
  hPt: number;
  onClose: () => void;
  onExport: () => void;
};

export class Viewer3D {
  readonly el: HTMLDivElement;
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private persp = new THREE.PerspectiveCamera();
  private ortho = new THREE.OrthographicCamera();
  private camera: THREE.Camera;
  private controls: OrbitControls;
  private lights: THREE.Object3D[] = [];
  private overlays = new THREE.Group();
  private edgeCache = new Map<string, THREE.BufferGeometry>();
  private viewIndex: number | null;
  private mode = "Solid";
  private lighting = "Artwork";
  private hidden = new Set<string>();
  private clip: THREE.Plane[] = [];
  private section: { axis: 0 | 1 | 2; pos: number; flip: boolean } | null = null;
  private tree: HTMLDivElement;
  private sectionBar: HTMLDivElement;
  private frame = 0;
  private disposed = false;
  private fullscreenHost: HTMLDivElement | null = null;
  private home: HTMLElement | null = null;

  constructor(private opts: ViewerOptions) {
    const { annot, model } = opts;
    this.el = document.createElement("div");
    this.el.className = "threeDView";
    this.renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: false });
    this.renderer.setPixelRatio(Math.min(2, window.devicePixelRatio || 1));
    this.renderer.localClippingEnabled = true;
    this.el.appendChild(this.renderer.domElement);
    this.scene.add(model.root);
    this.scene.add(this.overlays);
    this.camera = this.persp;
    this.scene.add(this.persp, this.ortho);
    this.controls = new OrbitControls(this.persp, this.renderer.domElement);
    this.controls.enableDamping = false;
    this.controls.screenSpacePanning = true;
    this.controls.addEventListener("change", () => this.requestRender());

    this.viewIndex = annot.default_view;
    const v = this.currentView();
    this.mode = v?.render_mode && RENDER_MODES.includes(v.render_mode) ? v.render_mode : "Solid";
    this.lighting = v?.lighting && LIGHTING_NAMES.includes(v.lighting) ? v.lighting : "Artwork";

    this.el.appendChild(this.buildToolbar());
    this.tree = this.buildTree();
    // On the page the tree would cover the model; /3DA /NP opens it in
    // fullscreen.
    this.tree.hidden = true;
    this.el.appendChild(this.tree);
    this.sectionBar = this.buildSectionBar();
    this.sectionBar.hidden = true;
    this.el.appendChild(this.sectionBar);
    if (model.header.warnings.length) this.el.title = model.header.warnings.join("\n");

    // Keep page tools and page scrolling out of the 3D view.
    for (const type of ["pointerdown", "mousedown", "click", "dblclick", "contextmenu"]) {
      this.el.addEventListener(type, (e) => e.stopPropagation());
    }
    this.renderer.domElement.addEventListener("wheel", (e) => e.preventDefault(), { passive: false });
    this.onKey = this.onKey.bind(this);
    window.addEventListener("keydown", this.onKey);
  }

  private currentView(): ThreeDView | null {
    const views = this.opts.annot.views;
    return this.viewIndex !== null && views[this.viewIndex] ? views[this.viewIndex] : null;
  }

  /** Size in CSS pixels; called whenever the page box moves or resizes. */
  resize(w: number, h: number) {
    if (this.disposed || w < 2 || h < 2) return;
    const s = this.renderer.getSize(new THREE.Vector2());
    if (Math.round(s.x) === Math.round(w) && Math.round(s.y) === Math.round(h)) return;
    const first = s.x === 0 || s.y === 0 || !this.started;
    this.renderer.setSize(w, h);
    if (first) {
      this.started = true;
      this.applyView(false);
      return;
    }
    // Keep the user's navigation; only the aspect ratio changes.
    const aspect = w / h;
    this.persp.aspect = aspect;
    this.persp.updateProjectionMatrix();
    const hh = this.ortho.top;
    this.ortho.left = -hh * aspect;
    this.ortho.right = hh * aspect;
    this.ortho.updateProjectionMatrix();
    this.requestRender();
  }

  private started = false;

  private aspect() {
    const s = this.renderer.getSize(new THREE.Vector2());
    return s.y > 0 ? s.x / s.y : 1;
  }

  /** Applies the current view: camera, background, mode, lighting, nodes,
   * cross sections. `full` = also reset mode/lighting to the view's. */
  applyView(full: boolean) {
    const v = this.currentView();
    const model = this.opts.model;
    if (full) {
      if (v?.render_mode && RENDER_MODES.includes(v.render_mode)) this.mode = v.render_mode;
      if (v?.lighting && LIGHTING_NAMES.includes(v.lighting)) this.lighting = v.lighting;
      this.hidden.clear();
      for (const n of v?.nodes ?? []) if (n.visible === false) this.hidden.add(n.name);
      this.section = null;
      this.syncTree();
      this.syncSelects();
    }
    const spec = resolveCamera(model, v, this.opts.wPt, this.opts.hPt);
    this.camera = applyCamera(spec, this.persp, this.ortho, this.aspect(), model.bounds);
    this.controls.object = this.camera;
    this.controls.target.set(...spec.target);
    this.controls.update();
    const bg = v?.background ?? [1, 1, 1];
    this.scene.background = srgb(bg[0], bg[1], bg[2]);
    this.applyLighting();
    this.applyMode();
    this.applyClipping();
    this.requestRender();
  }

  private applyLighting() {
    this.lights.forEach((l) => l.removeFromParent());
    this.lights = buildLights(this.lighting, this.opts.model, this.camera, this.scene);
  }

  private edgesFor(mesh: THREE.Mesh, kind: "wire" | "edges"): THREE.BufferGeometry {
    const key = `${kind}:${mesh.userData.geometry}`;
    let g = this.edgeCache.get(key);
    if (!g) {
      g = kind === "wire" ? new THREE.WireframeGeometry(mesh.geometry) : new THREE.EdgesGeometry(mesh.geometry, 45);
      this.edgeCache.set(key, g);
    }
    return g;
  }

  private applyMode() {
    const p = modeParts(this.mode);
    const v = this.currentView();
    const ac = v?.render_aux_color ?? [0, 0, 0];
    const bg = v?.background ?? [1, 1, 1];
    this.overlays.clear();
    const lineMat = new THREE.LineBasicMaterial({ color: srgb(ac[0], ac[1], ac[2]), clippingPlanes: this.clip });
    for (const mesh of this.opts.model.meshes) {
      const visible = !mesh.userData.path.some((n: string) => this.hidden.has(n));
      const base = mesh.userData.material as THREE.MeshPhongMaterial;
      let mat: THREE.Material = base;
      if (p.flat) {
        mat = new THREE.MeshBasicMaterial({ color: srgb(bg[0], bg[1], bg[2]), side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1 });
      } else if (p.hiddenLine) {
        mat = new THREE.MeshBasicMaterial({ colorWrite: false, side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1 });
      } else if (p.transparent) {
        mat = base.clone();
        mat.transparent = true;
        mat.opacity = base.opacity * 0.5;
        mat.depthWrite = false;
      } else if (p.edges || p.wire) {
        mat = base.clone();
        mat.polygonOffset = true;
        mat.polygonOffsetFactor = 1;
        mat.polygonOffsetUnits = 1;
      }
      mat.clippingPlanes = this.clip;
      mesh.material = mat;
      mesh.visible = visible && (p.solid || p.hiddenLine || p.flat);
      if (!visible) continue;
      const attach = (o: THREE.Object3D) => {
        o.matrixAutoUpdate = false;
        o.matrix.copy(mesh.matrix);
        this.overlays.add(o);
      };
      if (p.wire) attach(new THREE.LineSegments(this.edgesFor(mesh, "wire"), lineMat));
      if (p.edges) attach(new THREE.LineSegments(this.edgesFor(mesh, "edges"), lineMat));
      if (p.points) {
        attach(new THREE.Points(mesh.geometry, new THREE.PointsMaterial({ color: srgb(ac[0], ac[1], ac[2]), size: 2, sizeAttenuation: false, clippingPlanes: this.clip })));
      }
      if (p.box) {
        const box = mesh.geometry.boundingBox ?? new THREE.Box3();
        attach(new THREE.Box3Helper(box, srgb(ac[0], ac[1], ac[2])));
      }
    }
    this.overlays.updateMatrixWorld(true);
  }

  private applyClipping() {
    const planes: THREE.Plane[] = [];
    const v = this.currentView();
    for (const s of v?.cross_sections ?? []) {
      const axis = s.orientation.findIndex((o) => o === null);
      const normal = new THREE.Vector3(axis === 1 ? -1 : 0, axis === 2 ? -1 : 0, 0);
      if (axis === 0 || axis < 0) normal.set(-1, 0, 0);
      const euler = new THREE.Euler(
        THREE.MathUtils.degToRad(s.orientation[0] ?? 0),
        THREE.MathUtils.degToRad(s.orientation[1] ?? 0),
        THREE.MathUtils.degToRad(s.orientation[2] ?? 0),
        "XYZ",
      );
      normal.applyEuler(euler).normalize();
      planes.push(new THREE.Plane().setFromNormalAndCoplanarPoint(normal, new THREE.Vector3(...s.center)));
    }
    if (this.section && this.opts.model.bounds) {
      const n = new THREE.Vector3();
      n.setComponent(this.section.axis, this.section.flip ? 1 : -1);
      const b = this.opts.model.bounds;
      const at = b.min[this.section.axis] + (b.max[this.section.axis] - b.min[this.section.axis]) * this.section.pos;
      const point = new THREE.Vector3();
      point.setComponent(this.section.axis, at);
      planes.push(new THREE.Plane().setFromNormalAndCoplanarPoint(n, point));
    }
    this.clip.length = 0;
    this.clip.push(...planes);
    this.applyMode();
  }

  requestRender() {
    if (this.frame || this.disposed) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      if (!this.disposed) this.renderer.render(this.scene, this.camera);
    });
  }

  // ---------- UI ----------

  private select(title: string, options: [string, string][], value: string, onChange: (v: string) => void) {
    const s = document.createElement("select");
    s.title = title;
    for (const [v, label] of options) {
      const o = document.createElement("option");
      o.value = v;
      o.textContent = label;
      s.appendChild(o);
    }
    s.value = value;
    s.addEventListener("change", () => onChange(s.value));
    return s;
  }

  private button(icon: string, title: string, onClick: () => void) {
    const b = document.createElement("button");
    b.type = "button";
    b.title = title;
    b.innerHTML = `<i data-lucide="${icon}"></i>`;
    b.addEventListener("click", onClick);
    return b;
  }

  private viewSelect!: HTMLSelectElement;
  private modeSelect!: HTMLSelectElement;
  private lightSelect!: HTMLSelectElement;

  private syncSelects() {
    if (this.viewSelect) this.viewSelect.value = this.viewIndex === null ? "" : String(this.viewIndex);
    if (this.modeSelect) this.modeSelect.value = this.mode;
    if (this.lightSelect) this.lightSelect.value = this.lighting;
  }

  private buildToolbar(): HTMLDivElement {
    const bar = document.createElement("div");
    bar.className = "threeDBar";
    const { annot } = this.opts;
    const views: [string, string][] = annot.views.map((v, i) => [String(i), v.name || `${t("3d.view")} ${i + 1}`]);
    if (annot.default_view === null) views.unshift(["", t("3d.viewDefault")]);
    this.viewSelect = this.select(t("3d.views"), views, annot.default_view === null ? "" : String(annot.default_view), (val) => {
      this.viewIndex = val === "" ? null : parseInt(val, 10);
      this.applyView(true);
    });
    const home = this.button("house", t("3d.home"), () => this.applyView(true));
    this.home = home;
    this.modeSelect = this.select(t("3d.renderMode"), RENDER_MODES.map((m) => [m, t(`3d.rm.${m}`)]), this.mode, (val) => {
      this.mode = val;
      this.applyMode();
      this.requestRender();
    });
    this.lightSelect = this.select(t("3d.lighting"), LIGHTING_NAMES.map((m) => [m, t(`3d.ls.${m}`)]), this.lighting, (val) => {
      this.lighting = val;
      this.applyLighting();
      this.requestRender();
    });
    const proj = this.button("box", t("3d.projection"), () => {
      const v = this.currentView();
      const spec = resolveCamera(this.opts.model, v, this.opts.wPt, this.opts.hPt);
      spec.orthographic = this.camera !== this.ortho;
      spec.position = this.camera.position.toArray() as Vec3;
      spec.target = this.controls.target.toArray() as Vec3;
      spec.up = this.camera.up.toArray() as Vec3;
      if (spec.orthographic) {
        const d = this.camera.position.distanceTo(this.controls.target);
        spec.orthoHalfHeight = d * Math.tan(((this.persp.fov || 30) * Math.PI) / 360);
      }
      this.camera = applyCamera(spec, this.persp, this.ortho, this.aspect(), this.opts.model.bounds);
      this.controls.object = this.camera;
      this.controls.update();
      this.applyLighting();
      this.requestRender();
    });
    const tree = this.button("list-tree", t("3d.tree"), () => {
      this.tree.hidden = !this.tree.hidden;
    });
    const section = this.button("scissors", t("3d.section"), () => {
      this.sectionBar.hidden = !this.sectionBar.hidden;
      this.section = this.sectionBar.hidden ? null : { axis: 0, pos: 0.5, flip: false };
      this.syncSectionBar();
      this.applyClipping();
      this.requestRender();
    });
    const exp = this.button("download", t("3d.export"), () => this.opts.onExport());
    const full = this.button("maximize-2", t("3d.fullscreen"), () => this.toggleFullscreen());
    const close = this.button("x", t("3d.close"), () => this.opts.onClose());
    bar.append(this.viewSelect, home, this.modeSelect, this.lightSelect, proj, tree, section, exp, full, close);
    return bar;
  }

  private sectionAxis!: HTMLSelectElement;
  private sectionRange!: HTMLInputElement;

  private syncSectionBar() {
    if (!this.section) return;
    this.sectionAxis.value = String(this.section.axis);
    this.sectionRange.value = String(Math.round(this.section.pos * 1000));
  }

  private buildSectionBar(): HTMLDivElement {
    const bar = document.createElement("div");
    bar.className = "threeDSection";
    this.sectionAxis = this.select(t("3d.sectionAxis"), [["0", "X"], ["1", "Y"], ["2", "Z"]], "0", (v) => {
      if (this.section) this.section.axis = parseInt(v, 10) as 0 | 1 | 2;
      this.applyClipping();
      this.requestRender();
    });
    this.sectionRange = document.createElement("input");
    this.sectionRange.type = "range";
    this.sectionRange.min = "0";
    this.sectionRange.max = "1000";
    this.sectionRange.title = t("3d.sectionPos");
    this.sectionRange.addEventListener("input", () => {
      if (this.section) this.section.pos = parseInt(this.sectionRange.value, 10) / 1000;
      this.applyClipping();
      this.requestRender();
    });
    const flip = this.button("flip-horizontal-2", t("3d.sectionFlip"), () => {
      if (this.section) this.section.flip = !this.section.flip;
      this.applyClipping();
      this.requestRender();
    });
    bar.append(this.sectionAxis, this.sectionRange, flip);
    return bar;
  }

  private treeBoxes = new Map<string, HTMLInputElement>();

  private syncTree() {
    for (const [name, box] of this.treeBoxes) box.checked = !this.hidden.has(name);
  }

  private buildTree(): HTMLDivElement {
    const panel = document.createElement("div");
    panel.className = "threeDTree";
    const nodes = this.opts.model.header.tree.filter((n) => n.kind === "group" || n.kind === "model");
    const children = new Map<string, string[]>();
    const known = new Set(nodes.map((n) => n.name));
    for (const n of nodes) {
      const parent = n.parents.find((p) => known.has(p)) ?? "";
      if (!children.has(parent)) children.set(parent, []);
      children.get(parent)!.push(n.name);
    }
    const title = document.createElement("div");
    title.className = "threeDTreeTitle";
    title.textContent = t("3d.tree");
    panel.appendChild(title);
    const checkbox = (name: string) => {
      const box = document.createElement("input");
      box.type = "checkbox";
      box.checked = !this.hidden.has(name);
      box.addEventListener("click", (e) => e.stopPropagation());
      box.addEventListener("change", () => {
        if (box.checked) this.hidden.delete(name);
        else this.hidden.add(name);
        this.applyMode();
        this.requestRender();
      });
      this.treeBoxes.set(name, box);
      return box;
    };
    // Groups are collapsible (large CAD models have hundreds of parts);
    // only the top level starts open.
    const build = (parent: string, depth: number): HTMLUListElement | null => {
      const list = children.get(parent);
      if (!list || depth > 32) return null;
      const ul = document.createElement("ul");
      for (const name of list) {
        const li = document.createElement("li");
        const sub = build(name, depth + 1);
        const label = document.createElement("label");
        label.append(checkbox(name), document.createTextNode(name || "—"));
        if (sub) {
          const det = document.createElement("details");
          det.open = depth === 0 && list.length <= 12;
          const summary = document.createElement("summary");
          summary.appendChild(label);
          det.append(summary, sub);
          li.appendChild(det);
        } else {
          li.appendChild(label);
        }
        ul.appendChild(li);
      }
      return ul;
    };
    const ul = build("", 0);
    if (ul) panel.appendChild(ul);
    return panel;
  }

  private onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && this.fullscreenHost) {
      e.preventDefault();
      this.toggleFullscreen();
    }
  }

  private placeholder: Comment | null = null;

  toggleFullscreen() {
    if (this.fullscreenHost) {
      this.placeholder?.replaceWith(this.el);
      this.placeholder = null;
      this.fullscreenHost.remove();
      this.fullscreenHost = null;
      this.el.classList.remove("full");
      const box = this.el.parentElement;
      if (box) this.resize(box.clientWidth, box.clientHeight);
      return;
    }
    const host = document.createElement("div");
    host.className = "threeDFullscreen";
    this.placeholder = document.createComment("3d");
    this.el.replaceWith(this.placeholder);
    host.appendChild(this.el);
    document.body.appendChild(host);
    this.fullscreenHost = host;
    if (this.opts.annot.model_tree) this.tree.hidden = false;
    this.el.classList.add("full");
    this.resize(host.clientWidth, host.clientHeight);
  }

  get isFullscreen() {
    return this.fullscreenHost !== null;
  }

  dispose() {
    this.disposed = true;
    if (this.frame) cancelAnimationFrame(this.frame);
    window.removeEventListener("keydown", this.onKey);
    this.fullscreenHost?.remove();
    this.controls.dispose();
    this.scene.remove(this.opts.model.root);
    for (const m of this.opts.model.meshes) m.material = m.userData.material;
    this.edgeCache.forEach((g) => g.dispose());
    this.renderer.dispose();
    this.renderer.forceContextLoss();
    this.el.remove();
    void this.home;
  }
}
