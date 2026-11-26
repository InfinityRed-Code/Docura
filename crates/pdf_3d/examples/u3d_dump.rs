//! Dev probe: print shaders, materials, lights and nodes of U3D files.
//!
//!     cargo run -p pdf_3d --example u3d_dump -- file.u3d...

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("read");
        match pdf_3d::u3d::parse(&bytes) {
            Ok(mut s) => {
                for m in &mut s.meshes {
                    println!(
                        "mesh {}: {} faces, {} positions, {} normals, {} diffuse, {} tex coords, shading {:?}",
                        m.name,
                        m.faces.len(),
                        m.positions.len(),
                        m.normals.len(),
                        m.diffuse.len(),
                        m.tex_coords.len(),
                        m.shading
                    );
                }
                s.meshes.clear();
                println!("{path}\n{}", serde_json::to_string_pretty(&s).unwrap());
            }
            Err(e) => println!("{path}: ERROR {e}"),
        }
    }
}
