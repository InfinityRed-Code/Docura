// Camera math for PDF 3D views (ISO 32000-1 13.6.4 / 13.6.5), free of
// three.js so the inserter and the viewer share one definition.
//
// A PDF view's /C2W is a 3x4 camera-to-world matrix: columns x, y, z and the
// camera position; the camera looks along +z. The standard text says +y is
// "up", which only holds for a mirrored matrix; writers (Acrobat, media9)
// use proper rotations whose y axis points *down* on screen. The handedness
// of the matrix tells which convention a file uses.

import type { ThreeDView } from "./types";

export type Vec3 = [number, number, number];

export type CameraSpec = {
  position: Vec3;
  target: Vec3;
  up: Vec3;
  orthographic: boolean;
  /** Vertical field of view in degrees (perspective). */
  fovV: number;
  /** Half of the visible height in world units (orthographic). */
  orthoHalfHeight: number;
};

export type Bounds = { min: Vec3; max: Vec3 };

export const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
export const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
export const scale = (a: Vec3, s: number): Vec3 => [a[0] * s, a[1] * s, a[2] * s];
export const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
export const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
export const len = (a: Vec3) => Math.hypot(a[0], a[1], a[2]);
export const norm = (a: Vec3): Vec3 => {
  const l = len(a);
  return l > 0 ? scale(a, 1 / l) : [0, 0, 1];
};

export function center(b: Bounds): Vec3 {
  return scale(add(b.min, b.max), 0.5);
}

export function radius(b: Bounds): number {
  return Math.max(1e-6, len(sub(b.max, b.min)) / 2);
}

/** Vertical FOV from a PDF FOV spanning `span` ("W", "H", "Min", "Max" or a
 * diameter in points) of a `w` x `h` view box. */
export function verticalFov(fov: number, span: string | null | undefined, w: number, h: number): number {
  const f = Math.min(179, Math.max(1, fov));
  const t = Math.tan((f * Math.PI) / 360);
  let across = w; // what the FOV circle's diameter maps to
  switch (span ?? "W") {
    case "H": across = h; break;
    case "Min": across = Math.min(w, h); break;
    case "Max": across = Math.max(w, h); break;
    case "W": across = w; break;
    default: {
      const d = parseFloat(span ?? "");
      across = Number.isFinite(d) && d > 0 ? d : w;
    }
  }
  return (Math.atan((t * h) / across) * 360) / Math.PI;
}

/** Camera for a PDF view. `cameras` are the U3D view nodes (for /U3DPath);
 * `w`/`h` the annotation box in points. Returns null when the view does not
 * define a camera (the caller fits the model). */
export function cameraForView(
  v: ThreeDView,
  bounds: Bounds | null,
  w: number,
  h: number,
  cameras: { name: string; matrix: number[]; orthographic: boolean; projection: number }[],
): CameraSpec | null {
  let position: Vec3;
  let forward: Vec3;
  let up: Vec3;
  if (v.c2w && v.c2w.length === 12) {
    const m = v.c2w;
    const x: Vec3 = [m[0], m[1], m[2]];
    const y: Vec3 = [m[3], m[4], m[5]];
    const z: Vec3 = [m[6], m[7], m[8]];
    position = [m[9], m[10], m[11]];
    forward = norm(z);
    up = dot(x, cross(y, z)) >= 0 ? norm(scale(y, -1)) : norm(y);
  } else if (v.u3d_path.length > 0) {
    const name = v.u3d_path[v.u3d_path.length - 1];
    const cam = cameras.find((c) => c.name === name);
    if (!cam) return null;
    const m = cam.matrix;
    position = [m[12], m[13], m[14]];
    forward = norm([-m[8], -m[9], -m[10]]);
    up = norm([m[4], m[5], m[6]]);
  } else {
    return null;
  }
  // Orbit target: /CO along the view direction, else the point of the
  // model's center closest to the line of sight.
  let dist = v.center_of_orbit ?? 0;
  if (!(dist > 0)) {
    dist = bounds ? Math.max(dot(sub(center(bounds), position), forward), radius(bounds) * 0.1) : 1;
  }
  const target = add(position, scale(forward, dist));
  let orthoHalfHeight = 1;
  if (v.orthographic) {
    const os = v.ortho_scale && v.ortho_scale > 0 ? v.ortho_scale : 1;
    // Absolute binding: one camera unit is `os` points on the page.
    switch (v.ortho_binding ?? "Absolute") {
      case "W": orthoHalfHeight = (h / w) / os / 2; break;
      case "H": orthoHalfHeight = 1 / os / 2; break;
      case "Min": orthoHalfHeight = (h / Math.min(w, h)) / os / 2; break;
      case "Max": orthoHalfHeight = (h / Math.max(w, h)) / os / 2; break;
      default: orthoHalfHeight = h / os / 2;
    }
  }
  return {
    position,
    target,
    up,
    orthographic: v.orthographic,
    fovV: verticalFov(v.fov ?? 90, v.fov_span, w, h),
    orthoHalfHeight,
  };
}

/** A camera looking at the whole model from `dir` (from the model towards
 * the camera) with the given up vector. */
export function fitCamera(bounds: Bounds, dir: Vec3, up: Vec3, fovV: number, aspect: number): CameraSpec {
  const c = center(bounds);
  const r = radius(bounds);
  const halfV = (fovV * Math.PI) / 360;
  const halfH = Math.atan(Math.tan(halfV) * aspect);
  const dist = (r / Math.sin(Math.min(halfV, halfH))) * 1.02;
  return {
    position: add(c, scale(norm(dir), dist)),
    target: c,
    up,
    orthographic: false,
    fovV,
    orthoHalfHeight: r,
  };
}

/** Preset views for an inserted model (Z up), written as /C2W views. */
export function standardViews(bounds: Bounds, names: Record<string, string>, w: number, h: number): ThreeDView[] {
  const fov = 30;
  const presets: [string, Vec3, Vec3][] = [
    ["iso", [1, -1, 0.8], [0, 0, 1]],
    ["front", [0, -1, 0], [0, 0, 1]],
    ["back", [0, 1, 0], [0, 0, 1]],
    ["left", [-1, 0, 0], [0, 0, 1]],
    ["right", [1, 0, 0], [0, 0, 1]],
    ["top", [0, 0, 1], [0, 1, 0]],
    ["bottom", [0, 0, -1], [0, -1, 0]],
  ];
  const fovV = verticalFov(fov, "Min", w, h);
  return presets.map(([key, dir, up]) => {
    const cam = fitCamera(bounds, dir, up, fovV, w / h);
    const forward = norm(sub(cam.target, cam.position));
    const right = norm(cross(forward, up));
    const down = norm(cross(forward, right)); // y column: down on screen
    return {
      name: names[key] ?? key,
      internal_name: key,
      c2w: [...right, ...down, ...forward, ...cam.position],
      u3d_path: [],
      center_of_orbit: len(sub(cam.target, cam.position)),
      orthographic: false,
      fov,
      fov_span: "Min",
      ortho_scale: null,
      ortho_binding: null,
      background: [1, 1, 1],
      render_mode: "Solid",
      render_aux_color: null,
      lighting: "CAD",
      cross_sections: [],
      nodes: [],
    };
  });
}

/** Lighting schemes of ISO 32000-1 Table 310: directional lights in camera
 * space (x right, y up, looking +z) plus an ambient term. A zero direction
 * means a light along the line of sight. */
export const LIGHTING: Record<string, { lights: { color: Vec3; dir: Vec3 }[]; ambient: Vec3 }> = {
  White: {
    ambient: [0, 0, 0],
    lights: [
      { color: [0.38, 0.38, 0.45], dir: [-2, -1.5, -0.5] },
      { color: [0.6, 0.6, 0.67], dir: [2, 1.1, -2.5] },
      { color: [0.5, 0.5, 0.57], dir: [-0.5, 0, 2] },
    ],
  },
  Day: {
    ambient: [0, 0, 0],
    lights: [
      { color: [0.5, 0.5, 0.5], dir: [-2, -1.5, -0.5] },
      { color: [0.8, 0.8, 0.9], dir: [2, 1.1, -2.5] },
      { color: [0.9, 0.9, 0.9], dir: [0.02, 0.01, 2] },
    ],
  },
  Night: {
    ambient: [0, 0, 0],
    lights: [
      { color: [1, 0.75, 0.39], dir: [-2, -1.5, -0.5] },
      { color: [0.31, 0.47, 0.55], dir: [2, 1.1, -2.5] },
      { color: [0.5, 0.5, 1], dir: [0, 0, 2] },
    ],
  },
  Hard: {
    ambient: [0.5, 0.5, 0.5],
    lights: [
      { color: [0.5, 0.5, 0.5], dir: [-1.5, -1.5, -1.5] },
      { color: [0.8, 0.8, 0.9], dir: [1.5, 1.5, -1.5] },
      { color: [0.9, 0.9, 0.9], dir: [-0.5, 0, 2] },
    ],
  },
  Primary: {
    ambient: [0, 0, 0],
    lights: [
      { color: [1, 0.2, 0.5], dir: [-2, -1.5, -0.5] },
      { color: [0.2, 1, 0.5], dir: [2, 1.1, -2.5] },
      { color: [0, 0, 1], dir: [0, 0, 2] },
    ],
  },
  Blue: {
    ambient: [0, 0, 0],
    lights: [
      { color: [0.4, 0.4, 0.7], dir: [-2, -1.5, -0.5] },
      { color: [0.75, 0.75, 0.95], dir: [2, 1.1, -2.5] },
      { color: [0.7, 0.7, 0.95], dir: [0, 0, 2] },
    ],
  },
  Red: {
    ambient: [0, 0, 0],
    lights: [
      { color: [0.8, 0.3, 0.4], dir: [-2, -1.5, -0.5] },
      { color: [0.95, 0.5, 0.7], dir: [2, 1.1, -2.5] },
      { color: [0.95, 0.4, 0.5], dir: [0, 0, 2] },
    ],
  },
  Cube: {
    ambient: [0, 0, 0],
    lights: [
      [1, 0.01, 0.01], [0.01, 1, 0.01], [0.01, 0.01, 1],
      [-1, 0.01, 0.01], [0.01, -1, 0.01], [0.01, 0.01, -1],
    ].map((d) => ({ color: [0.4, 0.4, 0.4] as Vec3, dir: d as Vec3 })),
  },
  CAD: {
    ambient: [0, 0, 0],
    lights: [
      { color: [0.72, 0.72, 0.81], dir: [0, 0, 0] },
      { color: [0.2, 0.2, 0.2], dir: [-2, -1.5, -0.5] },
      { color: [0.32, 0.32, 0.32], dir: [2, 1.1, -2.5] },
      { color: [0.36, 0.36, 0.36], dir: [0.04, 0.01, 2] },
    ],
  },
  Headlamp: {
    ambient: [0.1, 0.1, 0.1],
    lights: [{ color: [0.8, 0.8, 0.9], dir: [0, 0, 0] }],
  },
};

export const LIGHTING_NAMES = ["Artwork", "None", "White", "Day", "Night", "Hard", "Primary", "Blue", "Red", "Cube", "CAD", "Headlamp"];

export const RENDER_MODES = [
  "Solid", "SolidWireframe", "Transparent", "TransparentWireframe", "Wireframe",
  "HiddenWireframe", "Illustration", "SolidOutline", "ShadedIllustration", "Vertices",
  "ShadedVertices", "BoundingBox", "TransparentBoundingBox", "TransparentBoundingBoxOutline",
  "ShadedWireframe",
];
