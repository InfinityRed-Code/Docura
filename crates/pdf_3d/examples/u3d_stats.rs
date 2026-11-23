//! Dev probe: decode .u3d files and print what came out.
//!
//!     cargo run -p pdf_3d --example u3d_stats -- file.u3d...

use pdf_3d::{render, u3d};

fn main() {
    for path in std::env::args().skip(1) {
        let name = std::path::Path::new(&path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let bytes = std::fs::read(&path).expect("read");
        let t = std::time::Instant::now();
        match u3d::parse(&bytes) {
            Ok(s) => {
                let tris: usize = render::flatten(&s)
                    .iter()
                    .map(|b| b.positions.len() / 9)
                    .sum();
                let meshes: Vec<String> = s
                    .meshes
                    .iter()
                    .map(|m| {
                        format!(
                            "{} {}/{}p {}/{}f {}n",
                            m.name,
                            m.positions.len(),
                            m.max_positions,
                            m.faces.len(),
                            m.max_faces,
                            m.normals.len()
                        )
                    })
                    .collect();
                println!(
                    "{name}: {:?} tris={tris} tex={} [{}] {:?}",
                    t.elapsed(),
                    s.textures.len(),
                    meshes.join(", "),
                    s.warnings
                );
            }
            Err(e) => println!("{name}: ERROR {e}"),
        }
    }
}
