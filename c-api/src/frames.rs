//! Per-page coordinate frames, read once at open from the loaded document.
//!
//! The sheet frame is the visible page box as laid out (`/Rotate` not
//! applied); the display frame is that box turned clockwise by the
//! inheritable `/Rotate`. Both are presented top-left, y down.

use super::output::flip_box;
use super::{Failure, Fallible, PdfBox, PdfPageInfo, PdfPoint};
use lopdf::Document;
use pdf_inspector::extractor::display_frame::{page_rotate, PageRotate};
use pdf_inspector::extractor::{visible_page_box, PageBox};
use pdf_inspector::PositionFrame;

#[derive(Clone, Copy, Debug)]
pub(super) struct PageFrame {
    /// 1-indexed page.
    pub page: u32,
    /// Visible box in raw user space, with its origin.
    pub sheet: PageBox,
    /// The page's `/Rotate`, snapped to a right angle.
    pub rotate: PageRotate,
    pub unit: f32,
}

impl PageFrame {
    /// Every page's frame in document order. A page without a usable box
    /// falls back to Letter, as the core's own frame conversion does.
    pub(super) fn all(doc: &Document) -> Fallible<Vec<PageFrame>> {
        let mut frames: Vec<PageFrame> = doc
            .get_pages()
            .into_iter()
            .map(|(page, id)| {
                Ok(PageFrame {
                    page,
                    sheet: visible_page_box(doc, id).unwrap_or(PageBox::LETTER),
                    rotate: page_rotate(doc, id),
                    unit: user_unit(doc, id)?,
                })
            })
            .collect::<Fallible<_>>()?;
        frames.sort_by_key(|frame| frame.page);
        Ok(frames)
    }
    /// Height of the requested frame, for the y-down presentation flip.
    pub(super) fn height(&self, frame: PositionFrame) -> f32 {
        match frame {
            PositionFrame::Sheet => self.sheet.height(),
            PositionFrame::Display => self.rotate.display_size(&self.sheet).1,
        }
    }
    /// Page dimensions in the requested frame; `rotation` is the applied `/Rotate`.
    pub(super) fn info(&self, frame: PositionFrame) -> PdfPageInfo {
        let (width, height) = match frame {
            PositionFrame::Sheet => (self.sheet.width(), self.sheet.height()),
            PositionFrame::Display => self.rotate.display_size(&self.sheet),
        };
        PdfPageInfo {
            page: self.page,
            width,
            height,
            rotation: self.rotate.degrees() as u32,
            user_unit: self.unit,
        }
    }
    /// A raw user-space box (lower-left origin, y up) presented in the
    /// requested frame, top-left origin, y down.
    pub(super) fn user_box_to_view(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        frame: PositionFrame,
    ) -> PdfBox {
        let (x, y) = (x - self.sheet.x0, y - self.sheet.y0);
        let (x, y, width, height) = match frame {
            PositionFrame::Sheet => (x, y, width, height),
            PositionFrame::Display => {
                self.rotate
                    .sheet_box_to_display(&self.sheet, x, y, width, height)
            }
        };
        flip_box(x, y, width, height, self.height(frame))
    }

    pub(super) fn user_point_to_view(&self, x: f32, y: f32, frame: PositionFrame) -> PdfPoint {
        let bounds = self.user_box_to_view(x, y, 0.0, 0.0, frame);
        PdfPoint {
            x: bounds.x0,
            y: bounds.y0,
        }
    }
}

fn user_unit(doc: &Document, id: lopdf::ObjectId) -> Fallible<f32> {
    let page = doc
        .get_dictionary(id)
        .map_err(|e| Failure::parse(e.to_string()))?;
    let value = match page.get(b"UserUnit") {
        Ok(value) => {
            doc.dereference(value)
                .map_err(|e| Failure::parse(e.to_string()))?
                .1
        }
        Err(_) => return Ok(1.0),
    };
    let unit = value
        .as_float()
        .map_err(|_| Failure::parse("invalid page UserUnit"))?;
    if !unit.is_finite() || unit <= 0.0 || unit > 75_000.0 {
        return Err(Failure::parse("invalid page UserUnit"));
    }
    Ok(unit)
}
