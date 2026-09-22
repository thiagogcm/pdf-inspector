//! Caller-hinted tables: TSR structure tokens plus cell quadrilaterals,
//! resolved by the core's structured-table readers.

use super::input::*;
use super::output::{narrow, page_box};
use super::*;
use std::collections::BTreeMap;

/// Column cap shared with the core table formatter.
const MAX_TABLE_COLUMNS: usize = 25;

/// Bound caller-supplied TSR structure before the core's lenient parser sees
/// it: one cell quadrilateral per cell tag, parseable spans, and spans that
/// cannot make the occupancy grid grow independently of the token count.
fn validate_structure_tokens(tokens: &[String], cells: usize) -> Fallible<()> {
    let rows = tokens
        .iter()
        .filter(|token| token.trim() == "<tr>")
        .count()
        .max(1);
    let mut cell_tags = 0usize;
    for token in tokens {
        let token = token.trim();
        match token {
            "<td></td>" | "<th></th>" | "<td" | "<th" => cell_tags += 1,
            _ => {
                let (name, limit) = if token.starts_with("rowspan") {
                    ("rowspan", rows)
                } else if token.starts_with("colspan") {
                    ("colspan", MAX_TABLE_COLUMNS)
                } else {
                    continue;
                };
                let value = token[name.len()..]
                    .trim()
                    .strip_prefix('=')
                    .map(|v| v.trim().trim_matches(|c| c == '"' || c == '\''))
                    .and_then(|v| v.parse::<usize>().ok());
                match value {
                    Some(v) if (1..=limit).contains(&v) => {}
                    _ => {
                        return Err(Failure::invalid(format!(
                            "invalid table structure: {name} must be an integer in 1..={limit}"
                        )))
                    }
                }
            }
        }
    }
    if cell_tags != cells {
        return Err(Failure::invalid("table structure and cell count differ"));
    }
    Ok(())
}

pub(super) fn validate_quad(q: PdfQuad) -> Fallible<()> {
    for p in q.points {
        finite(p.x, "polygon x")?;
        finite(p.y, "polygon y")?;
    }
    let area = q
        .points
        .iter()
        .zip(q.points.iter().cycle().skip(1))
        .take(4)
        .map(|(a, b)| f64::from(a.x) * f64::from(b.y) - f64::from(b.x) * f64::from(a.y))
        .sum::<f64>();
    if !area.is_finite() || area.abs() < f64::EPSILON {
        return Err(Failure::invalid("polygon must have positive area"));
    }
    Ok(())
}

pub(super) unsafe fn table_input(
    t: &PdfTableInput,
    count: u32,
) -> Fallible<pdf_inspector::TsrTableInput> {
    page(t.page, count)?;
    let crop = bounds(t.bounds)?;
    if t.mode > PDF_TSR_STRICT {
        return Err(Failure::invalid("invalid TSR mode"));
    }
    let tokens = slice(t.tokens.ptr, t.tokens.len)?
        .iter()
        .map(|s| text(*s).map(str::to_owned))
        .collect::<Fallible<Vec<_>>>()?;
    let cells = slice(t.cells.ptr, t.cells.len)?;
    validate_structure_tokens(&tokens, cells.len())?;
    let mut boxes = Vec::new();
    for cell in cells {
        validate_quad(*cell)?;
        boxes.push(
            cell.points
                .iter()
                .flat_map(|p| [p.x - crop[0], p.y - crop[1]])
                .collect(),
        );
    }
    Ok(pdf_inspector::TsrTableInput {
        page: t.page - 1,
        crop_pdf_pt_bbox: crop,
        render_dpi: 72.0,
        structure_tokens: tokens,
        cell_bboxes: boxes,
    })
}

/// Resolve every hinted table, in input order, with two batched core calls at most.
pub(super) fn hinted_tables(
    storage: &Storage,
    bytes: &[u8],
    tables: &[PdfTableInput],
    inputs: Vec<pdf_inspector::TsrTableInput>,
) -> Fallible<Vec<PdfTable>> {
    let cells = pdf_inspector::extract_tables_with_structure_cells_mem(bytes, &inputs)?;
    let auto_indices = tables
        .iter()
        .enumerate()
        .filter(|(_, t)| t.mode == PDF_TSR_AUTO)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    let auto_inputs = auto_indices
        .iter()
        .map(|&i| inputs[i].clone())
        .collect::<Vec<_>>();
    // Pair repairs with their descriptors by index; a short core answer
    // then surfaces as a missing entry rather than a misaligned one.
    let mut auto: BTreeMap<usize, _> = auto_indices
        .into_iter()
        .zip(pdf_inspector::extract_tables_with_structure_auto_mem(
            bytes,
            &auto_inputs,
        )?)
        .collect();
    if cells.len() != tables.len() || auto.len() != auto_inputs.len() {
        return Err(Failure::runtime("core answered fewer tables than queried"));
    }
    let mut views = Vec::with_capacity(tables.len());
    for (input_index, (input, cells)) in tables.iter().zip(cells).enumerate() {
        let (markdown, fallback) = match auto.remove(&input_index) {
            Some(r) => (r.markdown, r.fallback_reason),
            None => (pdf_inspector::tables::cells_to_markdown(&cells), None),
        };
        // A repaired table's cells no longer describe its Markdown.
        let resolved = if fallback.is_none() { &cells[..] } else { &[] };
        views.push(PdfTable {
            page: input.page,
            bounds: input.bounds,
            markdown: storage.owned(markdown.into_bytes()),
            fallback_reason: storage.optional(fallback.as_deref()),
            cells: storage.slice(resolved.iter().map(|c| PdfCell {
                row: narrow(c.row),
                column: narrow(c.col),
                row_span: narrow(c.rowspan),
                column_span: narrow(c.colspan),
                flags: u32::from(c.is_header) * PDF_CELL_HEADER,
                bounds: page_box(c.page_pt_bbox),
                text: storage.bytes(&c.text),
            })),
        });
    }
    Ok(views)
}
