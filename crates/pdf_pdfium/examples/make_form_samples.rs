//! Writes the synthetic form documents of the form scaling tests to a
//! directory, for a look in the app or in another viewer:
//!
//!   cargo run -p pdf_pdfium --example make_form_samples -- <out_dir>

#[path = "../tests/common/forms.rs"]
mod forms;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&dir).expect("create output directory");
    for (name, spec) in forms::variants() {
        let path = std::path::Path::new(&dir).join(format!("forms-{name}.pdf"));
        std::fs::write(&path, forms::form_pdf(spec)).expect("write sample");
        println!("{}", path.display());
    }
}
