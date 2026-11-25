//! Dev utility: decode a U3D file and write the packed scene the viewer
//! loads (`pdf_3d::render::pack`), as the app's `load_3d_scene` returns it.
//!
//!     cargo run -p pdf_3d --example u3d_pack -- in.u3d out.d3s
//!
//! Used by the UI's browser tests (apps/desktop-tauri/ui/tests/browser).

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: u3d_pack in.u3d out.d3s");
        std::process::exit(2);
    };
    let bytes = std::fs::read(&input).unwrap_or_else(|e| panic!("{input}: {e}"));
    let scene = pdf_3d::u3d::parse(&bytes).unwrap_or_else(|e| panic!("{input}: {e}"));
    let packed = pdf_3d::render::pack(&pdf_3d::render::prepare(&scene));
    std::fs::write(&output, packed).unwrap_or_else(|e| panic!("{output}: {e}"));
}
