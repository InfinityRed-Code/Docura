//! Dev utility: build a small AES-256-encrypted sample PDF.
//!
//! Usage: cargo run -p pdf_mutate --example make_encrypted -- <out.pdf> <user-pw> [owner-pw] [pages]

use lopdf::{Dictionary, Document, Object};

fn sample_pdf(n_pages: u32) -> Vec<u8> {
    let mut d = Document::with_version("1.5");
    let pages_id = d.new_object_id();
    let font_id = d.new_object_id();

    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    d.objects.insert(font_id, Object::Dictionary(font));

    let mut kids: Vec<Object> = Vec::new();
    let mut last_id = font_id;
    for i in 0..n_pages {
        let content_id = d.new_object_id();
        let page_id = d.new_object_id();
        let content = lopdf::Stream::new(
            Dictionary::new(),
            format!(
                "BT /F1 24 Tf 72 700 Td (rustpdf encrypted sample - page {n}) Tj ET\n\
                 BT /F1 12 Tf 72 660 Td (If you can read this, the password worked.) Tj ET",
                n = i + 1
            )
            .into_bytes(),
        );
        d.objects.insert(content_id, Object::Stream(content));

        let mut res = Dictionary::new();
        let mut fonts = Dictionary::new();
        fonts.set("F1", Object::Reference(font_id));
        res.set("Font", Object::Dictionary(fonts));

        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set("Contents", Object::Reference(content_id));
        page.set("Resources", Object::Dictionary(res));
        page.set(
            "MediaBox",
            Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        );
        d.objects.insert(page_id, Object::Dictionary(page));
        kids.push(Object::Reference(page_id));
        last_id = page_id;
    }

    let cat_id = d.new_object_id();
    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Kids", Object::Array(kids));
    pages.set("Count", Object::Integer(n_pages as i64));
    d.objects.insert(pages_id, Object::Dictionary(pages));

    let mut cat = Dictionary::new();
    cat.set("Type", Object::Name(b"Catalog".to_vec()));
    cat.set("Pages", Object::Reference(pages_id));
    d.objects.insert(cat_id, Object::Dictionary(cat));
    d.trailer.set("Root", Object::Reference(cat_id));
    d.max_id = cat_id.0.max(last_id.0);

    let mut out = Vec::new();
    d.save_to(&mut out).unwrap();
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "encrypted-sample.pdf".into());
    let user_pw = args.next().unwrap_or_else(|| "test".into());
    let owner_pw = args.next().unwrap_or_default();
    let n_pages: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);

    let plain = sample_pdf(n_pages);
    let perms = pdf_mutate::PermissionOptions {
        print: true,
        modify: false,
        copy: false,
        annotate: true,
    };
    let enc = pdf_mutate::encrypt_pdf(&plain, &user_pw, &owner_pw, perms).expect("encrypt");
    std::fs::write(&out, &enc).expect("write");
    println!("wrote {out} (user-pw: {user_pw:?}, owner-pw: {:?}, perms: {perms:?})",
        // Restricted perms + empty owner password => encrypt_pdf picks a random one.
        if owner_pw.is_empty() { "<random>" } else { owner_pw.as_str() });
}
