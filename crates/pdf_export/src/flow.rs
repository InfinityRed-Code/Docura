//! Text-flow reconstruction from positioned text runs.
//!
//! PDFs carry no paragraph structure — only glyphs with coordinates. The
//! heuristics here turn one page's `TextRun`s (already merged into line
//! fragments by the engine) into lines, then paragraphs, then blocks:
//!
//! 1. Runs whose vertical extents overlap by more than half the smaller
//!    height belong to the same line (sorted left-to-right).
//! 2. Consecutive lines join into one paragraph while the vertical gap
//!    stays under ~0.85× the median line height.
//! 3. A single-line paragraph whose height clearly exceeds the median
//!    (≥1.25×) is treated as a heading.

use pdf_types::TextRun;

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(String),
    Paragraph(String),
}

#[derive(Debug, Clone)]
pub struct Line {
    pub text: String,
    pub y0: f32,
    pub y1: f32,
    pub x0: f32,
}

/// Strip characters that XML 1.0 forbids — C0 controls except tab/LF/CR,
/// plus the non-characters U+FFFE/U+FFFF. Word and Excel refuse files
/// containing them. Defense in depth: the engine filters these too, but
/// runs may come from other sources.
pub(crate) fn sanitize_xml(s: &str) -> String {
    s.chars()
        .filter(|&c| (c >= '\u{20}' || c == '\t' || c == '\n' || c == '\r') && c != '\u{FFFE}' && c != '\u{FFFF}')
        .collect()
}

/// Group a page's runs into reading-order lines.
pub fn group_lines(runs: &[TextRun]) -> Vec<Line> {
    let mut sorted: Vec<&TextRun> = runs
        .iter()
        .filter(|r| !r.text.trim().is_empty())
        .collect();
    sorted.sort_by(|a, b| {
        let ya = (a.rect_pt.y0 + a.rect_pt.y1) / 2.0;
        let yb = (b.rect_pt.y0 + b.rect_pt.y1) / 2.0;
        ya.partial_cmp(&yb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.rect_pt
                    .x0
                    .partial_cmp(&b.rect_pt.x0)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    let mut lines: Vec<(Vec<&TextRun>, f32, f32)> = Vec::new();
    for run in sorted {
        let (ry0, ry1) = (run.rect_pt.y0, run.rect_pt.y1);
        if let Some((members, y0, y1)) = lines.last_mut() {
            let overlap = ry1.min(*y1) - ry0.max(*y0);
            let min_h = (ry1 - ry0).min(*y1 - *y0).max(0.1);
            if overlap > 0.5 * min_h {
                members.push(run);
                *y0 = y0.min(ry0);
                *y1 = y1.max(ry1);
                continue;
            }
        }
        lines.push((vec![run], ry0, ry1));
    }

    lines
        .into_iter()
        .map(|(mut members, y0, y1)| {
            members.sort_by(|a, b| {
                a.rect_pt
                    .x0
                    .partial_cmp(&b.rect_pt.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let x0 = members
                .first()
                .map(|r| r.rect_pt.x0)
                .unwrap_or(0.0);
            let text = members
                .iter()
                .map(|r| sanitize_xml(r.text.trim()))
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            Line { text, y0, y1, x0 }
        })
        .filter(|l| !l.text.is_empty())
        .collect()
}

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return 12.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[v.len() / 2]
}

/// Reconstruct flowed blocks (headings + paragraphs) from a page's runs.
pub fn reconstruct_blocks(runs: &[TextRun]) -> Vec<Block> {
    let lines = group_lines(runs);
    if lines.is_empty() {
        return Vec::new();
    }
    let med_h = median(lines.iter().map(|l| l.y1 - l.y0).collect());

    // Group lines into paragraphs by vertical gap.
    let mut paras: Vec<Vec<&Line>> = Vec::new();
    for line in &lines {
        let new_para = match paras.last().and_then(|p| p.last()) {
            Some(prev) => {
                let gap = line.y0 - prev.y1;
                let prev_h = prev.y1 - prev.y0;
                let cur_h = line.y1 - line.y0;
                // Big gap, or a size jump (e.g. heading → body) starts fresh.
                gap > 0.85 * med_h || (cur_h - prev_h).abs() > 0.35 * med_h
            }
            None => true,
        };
        if new_para {
            paras.push(vec![line]);
        } else {
            paras.last_mut().unwrap().push(line);
        }
    }

    paras
        .into_iter()
        .map(|p| {
            let text = p
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let h = p
                .iter()
                .map(|l| l.y1 - l.y0)
                .fold(0.0f32, f32::max);
            if p.len() == 1 && h >= 1.25 * med_h {
                Block::Heading(text)
            } else {
                Block::Paragraph(text)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_types::RectPt;

    fn run(text: &str, x0: f32, y0: f32, h: f32) -> TextRun {
        TextRun {
            text: text.into(),
            rect_pt: RectPt {
                x0,
                y0,
                x1: x0 + text.len() as f32 * h * 0.5,
                y1: y0 + h,
            },
        }
    }

    #[test]
    fn lines_paragraphs_and_heading() {
        let runs = vec![
            run("Große Überschrift", 72.0, 60.0, 20.0),
            // paragraph 1: two lines, tight leading
            run("Erste Zeile des", 72.0, 100.0, 12.0),
            run("Absatzes hier.", 72.0, 114.0, 12.0),
            // big gap → paragraph 2; split into two runs on one line
            run("Zweiter", 72.0, 160.0, 12.0),
            run("Absatz.", 130.0, 160.0, 12.0),
        ];
        let blocks = reconstruct_blocks(&runs);
        assert_eq!(
            blocks,
            vec![
                Block::Heading("Große Überschrift".into()),
                Block::Paragraph("Erste Zeile des Absatzes hier.".into()),
                Block::Paragraph("Zweiter Absatz.".into()),
            ]
        );
    }

    #[test]
    fn empty_page_gives_no_blocks() {
        assert!(reconstruct_blocks(&[]).is_empty());
    }

    #[test]
    fn control_chars_are_stripped() {
        // PDFium emits C0 controls (e.g. U+0002) for unmapped glyphs —
        // they are invalid in OOXML and must not survive into blocks.
        let runs = vec![run("Preis\u{2}: 10\u{2} EUR", 72.0, 100.0, 12.0)];
        let blocks = reconstruct_blocks(&runs);
        assert_eq!(blocks, vec![Block::Paragraph("Preis: 10 EUR".into())]);
    }
}
