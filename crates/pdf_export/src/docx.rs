//! PDF → Word: flowed .docx from reconstructed blocks (one section per
//! page, separated by page breaks). Layout fidelity is explicitly not the
//! goal — the output is meant for editing the text further in Word.

use docx_rs::{BreakType, Docx, Paragraph, Run};
use pdf_types::TextRun;

use crate::flow::{reconstruct_blocks, Block};
use crate::ExportError;

/// Build a .docx from per-page text runs. Returns the file bytes.
pub fn build_docx(pages: &[Vec<TextRun>]) -> Result<Vec<u8>, ExportError> {
    let mut docx = Docx::new();
    let mut first = true;
    for runs in pages {
        if !first {
            docx = docx.add_paragraph(
                Paragraph::new().add_run(Run::new().add_break(BreakType::Page)),
            );
        }
        first = false;
        for block in reconstruct_blocks(runs) {
            let para = match block {
                // Sizes are half-points: 28 = 14 pt, 22 = 11 pt.
                Block::Heading(t) => Paragraph::new()
                    .add_run(Run::new().add_text(t).size(28).bold()),
                Block::Paragraph(t) => {
                    Paragraph::new().add_run(Run::new().add_text(t).size(22))
                }
            };
            docx = docx.add_paragraph(para);
        }
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    docx.build()
        .pack(&mut buf)
        .map_err(|e| ExportError::Build(format!("docx: {e}")))?;
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_types::RectPt;

    #[test]
    fn produces_a_zip_with_document_xml() {
        let runs = vec![TextRun {
            text: "Hallo Welt".into(),
            rect_pt: RectPt { x0: 72.0, y0: 100.0, x1: 200.0, y1: 112.0 },
        }];
        let bytes = build_docx(&[vec![], runs.clone(), runs]).unwrap();
        // .docx = ZIP container.
        assert_eq!(&bytes[..2], b"PK");
        assert!(bytes.len() > 500);
    }
}
