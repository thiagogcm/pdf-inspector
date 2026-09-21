use super::output::narrow;
use super::*;
use lopdf::Document;
use pdf_inspector::extractor::{PageFrameInfo, PositionedPageContent};
use pdf_inspector::markdown::{detect_tables_from_items, MarkdownDocumentContext};
use pdf_inspector::structure_tree::StructTree;
use pdf_inspector::tables::TableKind;
use pdf_inspector::{MarkdownOptions, PositionFrame};

/// Automatically detected native tables on the selected pages, through
/// the same detector the Markdown pipeline uses, including TOCs.
pub(super) fn detect(
    storage: &Storage,
    content: &PositionedPageContent,
    doc: Option<&Document>,
    selected: &[u32],
    state: &DocumentState,
    options: MarkdownOptions,
    frame: PositionFrame,
) -> Vec<PdfTable> {
    let tree = doc.and_then(StructTree::from_doc);
    let pages = doc.map(Document::get_pages).unwrap_or_default();
    let roles = tree.as_ref().map(|tree| tree.mcid_to_roles(&pages));
    let tagged_tables = tree
        .as_ref()
        .map(|tree| tree.extract_tables(&pages))
        .unwrap_or_default();
    detect_tables_from_items(
        content.items.clone(),
        options,
        &content.rects,
        &content.lines,
        MarkdownDocumentContext {
            page_thresholds: &content.thresholds,
            struct_roles: roles.as_ref(),
            struct_tables: &tagged_tables,
            page_count: state.count(),
            prefiltered_page_number_pages: None,
            prefiltered_page_number_mask: None,
            precomputed_chart_regions: None,
        },
    )
    .into_iter()
    .filter(|(page, _)| selected.contains(page))
    .map(|(page, table)| {
        let info = state.frame_info(page).ok();
        let (bounds, column_edges, row_edges, flags) = table_geometry(&table, info.as_ref(), frame);
        let cells = table.cells.iter().enumerate().flat_map(|(row, cells)| {
            cells.iter().enumerate().map(move |(column, text)| PdfCell {
                row: narrow(row),
                column: narrow(column),
                row_span: 1,
                column_span: 1,
                text: storage.bytes(text),
                ..PdfCell::default()
            })
        });
        PdfTable {
            page,
            flags,
            kind: match table.kind {
                TableKind::Data => PDF_TABLE_DATA,
                TableKind::Toc => PDF_TABLE_TOC,
            },
            bounds,
            markdown: storage.owned(pdf_inspector::tables::table_to_markdown(&table).into_bytes()),
            column_edges: storage.slice(column_edges),
            row_edges: storage.slice(row_edges),
            cells: storage.slice(cells),
            ..PdfTable::default()
        }
    })
    .collect()
}

fn table_geometry(
    table: &pdf_inspector::tables::Table,
    info: Option<&PageFrameInfo>,
    frame: PositionFrame,
) -> (PdfBox, Vec<f32>, Vec<f32>, u32) {
    let Some(info) = info else {
        return (PdfBox::default(), Vec::new(), Vec::new(), 0);
    };
    if table.columns.len() < 2 || table.rows.len() < 2 {
        return (PdfBox::default(), Vec::new(), Vec::new(), 0);
    }
    let x0 = *table.columns.first().unwrap();
    let x1 = *table.columns.last().unwrap();
    let y_lo = table.rows.iter().copied().fold(f32::INFINITY, f32::min);
    let y_hi = table.rows.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let y_mid = (y_lo + y_hi) * 0.5;
    let x_mid = (x0 + x1) * 0.5;
    let bounds = super::output::user_box_to_view(x0, y_lo, x1 - x0, y_hi - y_lo, info, frame);
    let column_edges = table
        .columns
        .iter()
        .map(|&x| super::output::user_x_to_view(x, y_mid, info, frame))
        .collect();
    let row_edges = table
        .rows
        .iter()
        .map(|&y| super::output::user_y_to_view(x_mid, y, info, frame))
        .collect();
    (bounds, column_edges, row_edges, PDF_TABLE_HAS_BOUNDS)
}
