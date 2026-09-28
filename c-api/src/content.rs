//! One positioned parse of the selected pages: runs, path geometry, and the
//! per-page signals the core's extraction engine reports alongside them.

use super::frames::PageFrame;
use super::{Failure, Fallible};
use lopdf::Document;
use pdf_inspector::extractor::display_frame::document_items_to_display_frame;
use pdf_inspector::extractor::{
    extract_positioned_text_impl, CoordinateFrame, TextExtractionOptions,
};
use pdf_inspector::tounicode::FontCMaps;
use pdf_inspector::{PageRotation, PdfLine, PdfRect, TextItem};
use std::collections::{HashMap, HashSet};

/// A font whose ToUnicode/CMap coverage had gaps in the parsed pages.
pub(super) struct CMapGap {
    pub font: String,
    pub codes: u32,
    pub interpolated: u32,
    pub unmapped: u32,
}

/// Positioned content in the sheet frame (visible page box, lower-left
/// origin, y up; predominantly rotated pages turned to read upright), until
/// [`PageContent::to_display`] moves it to the display frame.
pub(super) struct PageContent {
    pub items: Vec<TextItem>,
    pub rects: Vec<PdfRect>,
    pub lines: Vec<PdfLine>,
    /// Frames of pages whose text was predominantly rotated; absent pages
    /// are upright.
    pub rotations: HashMap<u32, PageRotation>,
    /// Pages that met fonts with unresolvable gid-encoded glyphs.
    pub gid_pages: HashSet<u32>,
    pub cmap_gaps: Vec<CMapGap>,
}

pub(super) fn parse(
    doc: &Document,
    pages: &HashSet<u32>,
    options: TextExtractionOptions,
) -> Fallible<PageContent> {
    let font_cmaps = FontCMaps::from_doc(doc);
    let ((items, rects, lines), _thresholds, gid_pages, rotations, coverage) =
        extract_positioned_text_impl(
            doc,
            &font_cmaps,
            Some(pages),
            options,
            None,
            CoordinateFrame::VisiblePageBox,
        )
        .map_err(Failure::from)?;
    let cmap_gaps = coverage
        .into_iter()
        .filter(|(_, stats)| stats.has_gaps())
        .map(|(font, stats)| CMapGap {
            font,
            codes: stats.codes,
            interpolated: stats.interpolated,
            unmapped: stats.unmapped,
        })
        .collect();
    Ok(PageContent {
        items,
        rects,
        lines,
        rotations,
        gid_pages,
        cmap_gaps,
    })
}

impl PageContent {
    /// Move every run, rectangle, and line segment from the sheet frame to
    /// the display frame: untwist the page turn, then apply `/Rotate`.
    pub(super) fn move_to_display(&mut self, doc: &Document, frames: &[PageFrame]) {
        document_items_to_display_frame(doc, &mut self.items, &self.rotations);
        let frame = |page: u32| {
            page.checked_sub(1)
                .and_then(|i| frames.get(i as usize))
                .copied()
        };
        for rect in &mut self.rects {
            let Some(frame) = frame(rect.page) else {
                continue;
            };
            let turn = self.rotations.get(&rect.page).copied();
            let (x, y, width, height) =
                to_display(&frame, turn, rect.x, rect.y, rect.width, rect.height);
            rect.x = x;
            rect.y = y;
            rect.width = width;
            rect.height = height;
        }
        for line in &mut self.lines {
            let Some(frame) = frame(line.page) else {
                continue;
            };
            let turn = self.rotations.get(&line.page).copied();
            for (x, y) in [(&mut line.x1, &mut line.y1), (&mut line.x2, &mut line.y2)] {
                let (dx, dy, _, _) = to_display(&frame, turn, *x, *y, 0.0, 0.0);
                *x = dx;
                *y = dy;
            }
        }
    }
}

fn to_display(
    frame: &PageFrame,
    turn: Option<PageRotation>,
    mut x: f32,
    mut y: f32,
    mut width: f32,
    mut height: f32,
) -> (f32, f32, f32, f32) {
    turn.unwrap_or(PageRotation::Upright)
        .unrotate_box(&mut x, &mut y, &mut width, &mut height);
    frame
        .rotate
        .sheet_box_to_display(&frame.sheet, x, y, width, height)
}
