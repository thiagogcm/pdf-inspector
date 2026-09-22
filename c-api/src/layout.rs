//! Column intervals and reading order of one page, from the core's column
//! detector and newspaper classifier.

use pdf_inspector::extractor::{
    detect_columns, group_into_lines, is_newspaper_layout, is_text_layout_item, TextLine,
};
use pdf_inspector::text_utils::effective_width;
use pdf_inspector::TextItem;

/// Column x-intervals on `page` and whether reading order is newspaper
/// (sequential columns) rather than tabular (Y-interleaved). `items` are
/// this page's items; `page_has_table` is the gate the detector uses.
pub(super) fn page_columns(
    items: &[TextItem],
    page: u32,
    page_has_table: bool,
) -> (Vec<(f32, f32)>, bool) {
    let columns = detect_columns(items, page, page_has_table);
    let intervals: Vec<(f32, f32)> = columns.iter().map(|c| (c.x_min, c.x_max)).collect();
    if columns.len() < 2 {
        return (intervals, false);
    }
    // Assign each run to the column it overlaps most; runs spanning several
    // columns (titles, rules) belong to none.
    let mut buckets: Vec<Vec<TextItem>> = vec![Vec::new(); columns.len()];
    for item in items.iter().filter(|item| is_text_layout_item(item)) {
        let left = item.x;
        let right = item.x + effective_width(item);
        let mut spans = 0;
        let mut best = 0;
        let mut best_overlap = f32::NEG_INFINITY;
        for (index, column) in columns.iter().enumerate() {
            let overlap = (right.min(column.x_max) - left.max(column.x_min)).max(0.0);
            if overlap > 0.0 {
                spans += 1;
            }
            if overlap > best_overlap {
                best_overlap = overlap;
                best = index;
            }
        }
        if spans > 1 {
            continue;
        }
        buckets[best].push(item.clone());
    }
    let per_column_lines: Vec<Vec<TextLine>> = buckets.into_iter().map(group_into_lines).collect();
    (intervals, is_newspaper_layout(&per_column_lines, &columns))
}
