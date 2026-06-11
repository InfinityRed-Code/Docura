//! PDF → Excel: table-heuristic .xlsx. One worksheet per page; lines
//! become rows, and column boundaries are clustered from the x-start
//! positions of all runs on the page (gap tolerance ~10 pt). Works for
//! visually tabular PDFs; running prose simply lands in column A.

use pdf_types::TextRun;
use rust_xlsxwriter::Workbook;

use crate::flow::group_lines;
use crate::ExportError;

const COL_TOL_PT: f32 = 10.0;

/// Cluster the x-start positions of all runs into column start positions.
fn column_starts(runs: &[TextRun]) -> Vec<f32> {
    let mut xs: Vec<f32> = runs
        .iter()
        .filter(|r| !r.text.trim().is_empty())
        .map(|r| r.rect_pt.x0)
        .collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut cols: Vec<f32> = Vec::new();
    for x in xs {
        match cols.last() {
            Some(&last) if x - last <= COL_TOL_PT => {}
            _ => cols.push(x),
        }
    }
    cols
}

fn column_for(cols: &[f32], x0: f32) -> usize {
    cols.iter()
        .rposition(|&c| x0 >= c - COL_TOL_PT)
        .unwrap_or(0)
}

/// Build an .xlsx from per-page text runs. Returns the file bytes.
pub fn build_xlsx(pages: &[Vec<TextRun>]) -> Result<Vec<u8>, ExportError> {
    let mut wb = Workbook::new();
    for (i, runs) in pages.iter().enumerate() {
        let sheet = wb.add_worksheet();
        sheet
            .set_name(format!("Seite {}", i + 1))
            .map_err(|e| ExportError::Build(format!("xlsx sheet: {e}")))?;
        let cols = column_starts(runs);
        let lines = group_lines(runs);
        // group_lines merges runs into joined text — for cell granularity we
        // need the runs themselves, grouped per line. Redo the row grouping
        // on raw runs keyed by the line bands.
        let mut row = 0u32;
        for line in &lines {
            let mut cells: Vec<(usize, String)> = Vec::new();
            for r in runs.iter().filter(|r| !r.text.trim().is_empty()) {
                let yc = (r.rect_pt.y0 + r.rect_pt.y1) / 2.0;
                if yc < line.y0 || yc > line.y1 {
                    continue;
                }
                let col = column_for(&cols, r.rect_pt.x0);
                let clean = crate::flow::sanitize_xml(r.text.trim());
                if clean.is_empty() {
                    continue;
                }
                match cells.iter_mut().find(|(c, _)| *c == col) {
                    Some((_, text)) => {
                        text.push(' ');
                        text.push_str(&clean);
                    }
                    None => cells.push((col, clean)),
                }
            }
            for (col, text) in cells {
                // Numbers as numbers so Excel can calculate with them.
                let normalized = text.replace(',', ".");
                if let Ok(num) = normalized.trim().parse::<f64>() {
                    sheet
                        .write_number(row, col as u16, num)
                        .map_err(|e| ExportError::Build(format!("xlsx cell: {e}")))?;
                } else {
                    sheet
                        .write_string(row, col as u16, &text)
                        .map_err(|e| ExportError::Build(format!("xlsx cell: {e}")))?;
                }
            }
            row += 1;
        }
    }
    wb.save_to_buffer()
        .map(|b| b.to_vec())
        .map_err(|e| ExportError::Build(format!("xlsx: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_types::RectPt;

    fn run(text: &str, x0: f32, y0: f32) -> TextRun {
        TextRun {
            text: text.into(),
            rect_pt: RectPt { x0, y0, x1: x0 + 40.0, y1: y0 + 12.0 },
        }
    }

    #[test]
    fn clusters_columns_and_rows() {
        // 2×2 table: columns at x≈72 and x≈220.
        let runs = vec![
            run("Name", 72.0, 100.0),
            run("Betrag", 220.0, 100.0),
            run("Miete", 72.5, 120.0),
            run("1200", 221.0, 120.0),
        ];
        let bytes = build_xlsx(&[runs]).unwrap();
        assert_eq!(&bytes[..2], b"PK");
        assert!(bytes.len() > 500);
        let cols = column_starts(&[
            run("a", 72.0, 0.0),
            run("b", 72.5, 0.0),
            run("c", 220.0, 0.0),
        ]);
        assert_eq!(cols.len(), 2);
    }
}
