use super::*;
use lopdf::Document;
use pdf_inspector::extractor::PositionedPageContent;
use pdf_inspector::markdown::{detect_data_tables_from_items, MarkdownDocumentContext};
use pdf_inspector::structure_tree::StructTree;
use pdf_inspector::MarkdownOptions;

/// Automatically detected native data tables on the selected pages, through
/// the same detector the Markdown pipeline uses.
pub(super) fn detect(
    storage: &mut Storage,
    content: &PositionedPageContent,
    doc: Option<&Document>,
    selected: &[u32],
    count: u32,
    options: MarkdownOptions,
) -> Vec<PdfTable> {
    let tree = doc.and_then(StructTree::from_doc);
    let pages = doc.map(Document::get_pages).unwrap_or_default();
    let roles = tree.as_ref().map(|tree| tree.mcid_to_roles(&pages));
    let tagged_tables = tree
        .as_ref()
        .map(|tree| tree.extract_tables(&pages))
        .unwrap_or_default();
    detect_data_tables_from_items(
        content.items.clone(),
        options,
        &content.rects,
        &content.lines,
        MarkdownDocumentContext {
            page_thresholds: &content.thresholds,
            struct_roles: roles.as_ref(),
            struct_tables: &tagged_tables,
            page_count: count,
            prefiltered_page_number_pages: None,
            prefiltered_page_number_mask: None,
            precomputed_chart_regions: None,
        },
    )
    .into_iter()
    .filter(|(page, _)| selected.contains(page))
    .map(|(page, table)| {
        // Detector coordinates mix centers and boundaries. Do not manufacture
        // boxes, header semantics, or merged-cell spans from them.
        let cells = table
            .cells
            .iter()
            .enumerate()
            .flat_map(|(row, cells)| {
                cells
                    .iter()
                    .enumerate()
                    .map(move |(column, text)| (row, column, text))
            })
            .map(|(row, column, text)| PdfCell {
                row,
                column,
                row_span: 1,
                column_span: 1,
                text: storage.bytes(text.as_bytes()),
                ..PdfCell::default()
            })
            .collect();
        PdfTable {
            page,
            markdown: storage.bytes(pdf_inspector::tables::table_to_markdown(&table).into_bytes()),
            cells: storage.cells(cells),
            ..PdfTable::default()
        }
    })
    .collect()
}
