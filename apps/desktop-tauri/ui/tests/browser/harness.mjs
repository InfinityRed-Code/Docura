// Browser test harness: serves the UI sources with Vite and drives a
// headless Chromium (Edge or Chrome) over the DevTools protocol, so tests
// run the real three.js viewer with WebGL. No dependency beyond Vite and
// Node >= 22 (global WebSocket). Tests skip when no browser is found; set
// DOCURA_TEST_BROWSER to a Chromium executable to choose one.

import { spawn, execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
export const uiRoot = path.resolve(here, "../..");
export const repoRoot = path.resolve(uiRoot, "../../..");

export function findBrowser() {
  const candidates = [
    process.env.DOCURA_TEST_BROWSER,
    "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
    "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
    "C:/Program Files/Google/Chrome/Application/chrome.exe",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "/usr/bin/microsoft-edge",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  ];
  return candidates.find((p) => p && fs.existsSync(p)) ?? null;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Starts Vite and a headless browser on the viewer test page. Returns
 * null when no browser is installed. */
export async function startViewerPage({ width = 900, height = 700 } = {}) {
  const browserPath = findBrowser();
  if (!browserPath) return null;
  const { createServer } = await import("vite");
  const server = await createServer({
    root: uiRoot,
    configFile: false,
    logLevel: "silent",
    server: { port: 0, strictPort: false, host: "127.0.0.1", hmr: false, watch: null },
  });
  await server.listen();
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`;

  const profile = fs.mkdtempSync(path.join(os.tmpdir(), "docura-viewer-test-"));
  const child = spawn(browserPath, [
    "--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`,
    "--no-first-run", "--no-default-browser-check", "--disable-extensions",
    "--force-color-profile=srgb", "--hide-scrollbars", "--force-device-scale-factor=1",
    "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist",
    `--window-size=${width},${height}`, "about:blank",
  ], { stdio: "ignore" });

  let ws;
  const close = async () => {
    try { ws?.close(); } catch { /* already closed */ }
    child.kill();
    await server.close();
    for (let i = 0; i < 30; i++) {
      try { fs.rmSync(profile, { recursive: true, force: true }); break; } catch { await sleep(100); }
    }
  };

  try {
    const portFile = path.join(profile, "DevToolsActivePort");
    // The browser writes the file while starting: it can be missing,
    // empty or still locked for a moment.
    let port = "";
    for (let i = 0; i < 400 && !port; i++) {
      try { port = fs.readFileSync(portFile, "utf8").split("\n")[0].trim(); } catch { /* not there yet */ }
      if (!port) await sleep(50);
    }
    if (!port) throw new Error("the browser did not open a DevTools port");
    let target;
    for (let i = 0; i < 100 && !target; i++) {
      const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      target = list.find((t) => t.type === "page");
      if (!target) await sleep(50);
    }
    ws = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
  } catch (e) {
    await close();
    throw e;
  }
  let id = 0;
  const pending = new Map();
  ws.onmessage = (m) => {
    const d = JSON.parse(m.data);
    if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); }
  };
  const send = (method, params = {}) => new Promise((resolve) => {
    const i = ++id;
    pending.set(i, resolve);
    ws.send(JSON.stringify({ id: i, method, params }));
  });

  /** Runs `body` (the body of an async function) in the page. */
  const evaluate = async (body) => {
    const r = await send("Runtime.evaluate", { expression: `(async () => { ${body} })()`, awaitPromise: true, returnByValue: true });
    const ex = r.result?.exceptionDetails;
    if (ex) throw new Error(ex.exception?.description ?? ex.text);
    return r.result?.result?.value;
  };

  await send("Page.enable");
  await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await send("Page.navigate", { url: `${origin}/tests/browser/harness.html` });
  // The first load lets Vite pre-bundle three.js, which can take a while.
  for (let i = 0; ; i++) {
    const ok = await evaluate("return window.viewerTestReady === true").catch(() => false);
    if (ok) break;
    if (i > 1200) { await close(); throw new Error("viewer test page did not load"); }
    await sleep(100);
  }

  return {
    evaluate,
    /** The page as the user would see it: a decoded RGBA screenshot. */
    async screenshot(clip) {
      const r = await send("Page.captureScreenshot", { format: "png", ...(clip ? { clip: { ...clip, scale: 1 } } : {}) });
      return decodePng(Buffer.from(r.result.data, "base64"));
    },
    close,
  };
}

const packed = new Map();

/** The packed viewer scene of a U3D file (base64), decoded by the Rust
 * crate exactly as the app's `load_3d_scene` does. */
export function packU3d(u3dPath) {
  const abs = path.resolve(repoRoot, u3dPath);
  if (!packed.has(abs)) {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "docura-d3s-"));
    const out = path.join(dir, "scene.d3s");
    execFileSync("cargo", ["run", "-q", "-p", "pdf_3d", "--example", "u3d_pack", "--", abs, out], { cwd: repoRoot, stdio: ["ignore", "ignore", "inherit"] });
    packed.set(abs, fs.readFileSync(out).toString("base64"));
    fs.rmSync(dir, { recursive: true, force: true });
  }
  return packed.get(abs);
}

/** Packs a scene description the way `pdf_3d::render::pack` does (base64).
 * `geometries`: { positions, normals?, colors?, uvs?, indices } as arrays;
 * the rest goes into the header as given. */
export function packScene({ geometries, instances, materials, textures = [], tree = [], cameras = [], lights = [], bounds = null, warnings = [], ...more }) {
  const chunks = [];
  let size = 0;
  const put = (buf) => {
    const pad = (4 - (size % 4)) % 4;
    if (pad) { chunks.push(Buffer.alloc(pad)); size += pad; }
    const range = { offset: size, length: buf.length };
    chunks.push(buf);
    size += buf.length;
    return range;
  };
  const f32 = (a) => Buffer.from(new Float32Array(a).buffer);
  const u32 = (a) => Buffer.from(new Uint32Array(a).buffer);
  const header = {
    geometries: geometries.map((g) => ({
      ...g,
      positions: put(f32(g.positions)),
      normals: g.normals ? put(f32(g.normals)) : null,
      colors: g.colors ? put(f32(g.colors)) : null,
      uvs: (g.uvs ?? []).map((set) => put(f32(set))),
      indices: put(u32(g.indices)),
    })),
    instances, materials,
    textures: textures.map((t) => ({ name: t.name, mime: t.mime, data: put(t.data) })),
    tree, cameras, lights, bounds, units_m: null, warnings,
    triangles: geometries.reduce((n, g) => n + g.indices.length / 3, 0),
    ...more,
  };
  const json = Buffer.from(JSON.stringify(header), "utf8");
  const head = Buffer.alloc(8);
  head.write("D3S2", 0, "latin1");
  head.writeUInt32LE(json.length, 4);
  const pad = Buffer.alloc((4 - ((8 + json.length) % 4)) % 4, 0x20);
  return Buffer.concat([head, json, pad, ...chunks]).toString("base64");
}

/** Decodes an 8-bit RGB/RGBA, non-interlaced PNG. */
export function decodePng(buf) {
  let p = 8;
  let width = 0, height = 0, type = 0;
  const idat = [];
  while (p < buf.length) {
    const len = buf.readUInt32BE(p);
    const name = buf.toString("latin1", p + 4, p + 8);
    const data = buf.subarray(p + 8, p + 8 + len);
    if (name === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      type = data[9];
      if (data[8] !== 8 || (type !== 2 && type !== 6) || data[12] !== 0) throw new Error("unsupported PNG");
    } else if (name === "IDAT") idat.push(data);
    p += 12 + len;
  }
  const bpp = type === 6 ? 4 : 3;
  const raw = zlib.inflateSync(Buffer.concat(idat));
  const stride = width * bpp;
  const out = Buffer.alloc(width * height * 4, 255);
  let prev = Buffer.alloc(stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = Buffer.from(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)));
    for (let x = 0; x < stride; x++) {
      const a = x >= bpp ? line[x - bpp] : 0, b = prev[x], c = x >= bpp ? prev[x - bpp] : 0;
      let v = 0;
      if (filter === 1) v = a;
      else if (filter === 2) v = b;
      else if (filter === 3) v = (a + b) >> 1;
      else if (filter === 4) {
        const pa = Math.abs(b - c), pb = Math.abs(a - c), pc = Math.abs(a + b - 2 * c);
        v = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      line[x] = (line[x] + v) & 255;
    }
    for (let x = 0; x < width; x++) {
      out[(y * width + x) * 4] = line[x * bpp];
      out[(y * width + x) * 4 + 1] = line[x * bpp + 1];
      out[(y * width + x) * 4 + 2] = line[x * bpp + 2];
      if (bpp === 4) out[(y * width + x) * 4 + 3] = line[x * bpp + 3];
    }
    prev = line;
  }
  const at = (x, y) => [out[(y * width + x) * 4], out[(y * width + x) * 4 + 1], out[(y * width + x) * 4 + 2]];
  return { width, height, data: out, at };
}

/** Compares two RGBA images of the same size showing a model on a plain
 * background (`a` and `b`: { width, height, data }). Background is the
 * color of the top middle pixel of each; `ignore(x, y)` masks pixels (e.g.
 * another viewer's axis gizmo). Colors are compared on interior pixels
 * only: model in both images and no edge within two pixels.
 * Returns deviations as fractions of the display range. */
export function compareImages(a, b, ignore = () => false) {
  const { width: w, height: h } = a;
  if (b.width !== w || b.height !== h) throw new Error("image sizes differ");
  const px = (img, x, y) => [img.data[(y * w + x) * 4], img.data[(y * w + x) * 4 + 1], img.data[(y * w + x) * 4 + 2]];
  const dist = (p, q) => Math.abs(p[0] - q[0]) + Math.abs(p[1] - q[1]) + Math.abs(p[2] - q[2]);
  const bgA = px(a, w >> 1, 3), bgB = px(b, w >> 1, 3);
  const inA = (x, y) => !ignore(x, y) && dist(px(a, x, y), bgA) > 20;
  const inB = (x, y) => !ignore(x, y) && dist(px(b, x, y), bgB) > 20;
  const bbox = (f) => {
    let x0 = w, y0 = h, x1 = -1, y1 = -1;
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) if (f(x, y)) { x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y); }
    return [x0, y0, x1, y1];
  };
  const diffs = [];
  let both = 0, either = 0;
  for (let y = 2; y < h - 2; y++) {
    for (let x = 2; x < w - 2; x++) {
      const ia = inA(x, y), ib = inB(x, y);
      if (ia || ib) either++;
      if (!ia || !ib) continue;
      both++;
      const pa = px(a, x, y), pb = px(b, x, y);
      let interior = true;
      for (let dy = -2; dy <= 2 && interior; dy++) {
        for (let dx = -2; dx <= 2; dx++) {
          if (!inA(x + dx, y + dy) || !inB(x + dx, y + dy) || dist(px(a, x + dx, y + dy), pa) > 30 || dist(px(b, x + dx, y + dy), pb) > 30) { interior = false; break; }
        }
      }
      if (interior) for (let k = 0; k < 3; k++) diffs.push(Math.abs(pa[k] - pb[k]) / 255);
    }
  }
  diffs.sort((p, q) => p - q);
  return {
    pixels: diffs.length / 3,
    mean: diffs.reduce((s, v) => s + v, 0) / (diffs.length || 1),
    p95: diffs[Math.floor(diffs.length * 0.95)] ?? 0,
    max: diffs.at(-1) ?? 0,
    backgroundA: bgA, backgroundB: bgB,
    bboxA: bbox(inA), bboxB: bbox(inB),
    overlap: either ? both / either : 1,
  };
}

/** Encodes RGBA pixels as a PNG (for textures of synthetic test scenes). */
export function encodePng(width, height, rgba) {
  const table = [];
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  const crc = (buf) => {
    let c = 0xffffffff;
    for (const x of buf) c = table[(c ^ x) & 255] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type, "latin1"), data]);
    const sum = Buffer.alloc(4);
    sum.writeUInt32BE(crc(body));
    return Buffer.concat([len, body, sum]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const raw = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++) Buffer.from(rgba.buffer, rgba.byteOffset + y * width * 4, width * 4).copy(raw, y * (width * 4 + 1) + 1);
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", ihdr), chunk("IDAT", zlib.deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}
