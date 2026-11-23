//! Spike: decode a .u3d file and write a standalone three.js page showing it.
//!
//!     cargo run -p pdf_3d --example u3d_viewer -- in.u3d out.html

use pdf_3d::{render, u3d};

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("usage: u3d_viewer in.u3d out.html");
    let output = args.next().expect("usage: u3d_viewer in.u3d out.html");
    let bytes = std::fs::read(&input).expect("read");
    let t = std::time::Instant::now();
    let scene = u3d::parse(&bytes).expect("parse");
    let batches = render::flatten(&scene);
    let tris: usize = batches.iter().map(|b| b.positions.len() / 9).sum();
    println!(
        "decoded in {:?}: {} nodes, {} meshes, {} batches, {} triangles, warnings {:?}",
        t.elapsed(),
        scene.nodes.len(),
        scene.meshes.len(),
        batches.len(),
        tris,
        scene.warnings
    );
    let json = serde_json::to_string(&batches).unwrap();
    let html = TEMPLATE
        .replace("__BATCHES__", &json)
        .replace("__TITLE__", &input);
    std::fs::write(&output, html).unwrap();
}

const TEMPLATE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>U3D spike</title>
<style>html,body{margin:0;height:100%;background:#e8edf3;font:13px system-ui}#info{position:absolute;left:8px;top:6px;color:#333}</style>
<script type="importmap">{"imports":{"three":"https://cdn.jsdelivr.net/npm/three@0.170.0/build/three.module.js","three/addons/":"https://cdn.jsdelivr.net/npm/three@0.170.0/examples/jsm/"}}</script>
</head><body><div id="info">__TITLE__</div>
<script type="module">
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
const batches = __BATCHES__;
const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
renderer.setSize(innerWidth, innerHeight);
renderer.setClearColor(0xe8edf3);
document.body.appendChild(renderer.domElement);
const scene = new THREE.Scene();
const root = new THREE.Group();
for (const b of batches) {
  const g = new THREE.BufferGeometry();
  g.setAttribute("position", new THREE.Float32BufferAttribute(b.positions, 3));
  g.setAttribute("normal", new THREE.Float32BufferAttribute(b.normals, 3));
  if (b.colors.length) g.setAttribute("color", new THREE.Float32BufferAttribute(b.colors, 3));
  const m = new THREE.MeshPhongMaterial({ color: new THREE.Color(b.color[0], b.color[1], b.color[2]),
    opacity: b.color[3], transparent: b.color[3] < 1, side: THREE.DoubleSide, vertexColors: b.colors.length > 0 });
  root.add(new THREE.Mesh(g, m));
}
scene.add(root);
scene.add(new THREE.HemisphereLight(0xffffff, 0x555566, 1.2));
const box = new THREE.Box3().setFromObject(root);
const c = box.getCenter(new THREE.Vector3()), r = box.getSize(new THREE.Vector3()).length() / 2 || 1;
const cam = new THREE.PerspectiveCamera(35, innerWidth / innerHeight, r / 100, r * 100);
cam.position.copy(c).add(new THREE.Vector3(1, -1.6, 1.2).normalize().multiplyScalar(r * 3.2));
cam.up.set(0, 0, 1);
const head = new THREE.DirectionalLight(0xffffff, 1.4); cam.add(head); scene.add(cam);
const controls = new OrbitControls(cam, renderer.domElement);
controls.target.copy(c); controls.update();
function loop() { renderer.render(scene, cam); requestAnimationFrame(loop); }
loop();
</script></body></html>
"#;
