use super::input::*;
use super::output::{flip_box, flip_point, page_box};
use super::*;
use lopdf::Document;
use pdf_inspector::extractor::PageFrameInfo;
use pdf_inspector::vision::{
    FusedPageMarkdown, ImagePoint, ImageQuad, ModelIdentity, OcrPage, OcrRun, OcrSpan,
    PageContentSource, PageTransform, RenderPixelFormat, RenderedPage, RoutedOcrPage,
};
use pdf_inspector::{DetectionConfig, PageMarkdown, PositionFrame, PositionOptions, ScanStrategy};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::time::Instant;

/// Immutable, decrypted document bytes plus per-page frames cached at open.
/// All processing runs the core's byte-oriented API against `bytes`.
pub(super) struct DocumentState {
    bytes: Vec<u8>,
    pub(super) count: u32,
    frames: Vec<PageFrameInfo>,
    audit: PdfLoadAudit,
}
impl DocumentState {
    pub(super) fn open(bytes: Vec<u8>, password: Option<&str>) -> Fallible<Self> {
        let (mut doc, count, repairs) =
            pdf_inspector::load_document_from_mem_with_repairs(&bytes, password)?;
        // Later operations reparse the bytes without a password, so a source
        // that needed one is re-serialized in its decrypted form once. The
        // loader already strips the encryption dictionary, so the raw bytes
        // are probed instead; a false positive only costs a rewrite.
        let encrypted = password.is_some_and(|p| !p.is_empty())
            && bytes.windows(8).any(|window| window == b"/Encrypt");
        let mut flags = 0;
        if encrypted {
            flags |= PDF_LOAD_DECRYPTED;
        }
        if repairs.leading_bytes > 0 {
            flags |= PDF_LOAD_LEADING_BYTES;
        }
        if repairs.container_repaired {
            flags |= PDF_LOAD_CONTAINER_REPAIRED;
        }
        if repairs.widened_form_bboxes > 0 {
            flags |= PDF_LOAD_WIDENED_FORM_BBOX;
        }
        let bytes = if encrypted {
            doc.encryption_state = None;
            doc.trailer.remove(b"Encrypt");
            let mut decrypted = Vec::new();
            doc.save_to(&mut decrypted).map_err(|e| Failure {
                status: PDF_PARSE_ERROR,
                message: format!("could not serialize decrypted document: {e}"),
            })?;
            pdf_inspector::load_document_from_mem_with_password(&decrypted, None)?;
            decrypted
        } else {
            bytes
        };
        // The loader already counted the widened forms; only a source that
        // needed widening is re-serialized (encrypted sources were just
        // serialized from the widened document).
        let bytes = if repairs.widened_form_bboxes > 0 && !encrypted {
            pdf_inspector::widen_degenerate_form_bboxes_mem(&bytes)?.unwrap_or(bytes)
        } else {
            bytes
        };
        // Frames come from the tested core helper on the final (decrypted
        // and form-repaired) bytes, so encrypted sources do not need a
        // password here.
        let frames =
            pdf_inspector::extractor::page_frame_info_mem(&bytes).map_err(Failure::from)?;
        Ok(Self {
            bytes,
            count,
            frames,
            audit: PdfLoadAudit {
                flags,
                leading_bytes: u32::try_from(repairs.leading_bytes).unwrap_or(u32::MAX),
                widened_form_bboxes: u32::try_from(repairs.widened_form_bboxes).unwrap_or(u32::MAX),
            },
        })
    }
    fn document(&self) -> Fallible<Document> {
        Ok(pdf_inspector::load_document_from_mem_with_password(&self.bytes, None)?.0)
    }
    pub(super) fn frame_info(&self, page: u32) -> Fallible<PageFrameInfo> {
        self.frames
            .iter()
            .find(|f| f.page == page)
            .copied()
            .ok_or_else(|| Failure::invalid(format!("page {page} is outside 1..={}", self.count)))
    }
    fn page_info(&self, page: u32, frame: PositionFrame) -> Fallible<PdfPageInfo> {
        let info = self.frame_info(page)?;
        let (width, height) = match frame {
            PositionFrame::Sheet => (info.sheet_width, info.sheet_height),
            PositionFrame::Display => (info.display_width, info.display_height),
        };
        Ok(PdfPageInfo {
            page,
            width,
            height,
            rotation: info.rotation_degrees,
        })
    }
    fn sheet_info(&self, page: u32) -> Fallible<PdfPageInfo> {
        let info = self.frame_info(page)?;
        Ok(PdfPageInfo {
            page,
            width: info.sheet_width,
            height: info.sheet_height,
            rotation: info.rotation_degrees,
        })
    }
}

const ALL_OUTPUTS: u32 = PDF_INSPECTION
    | PDF_MARKDOWN
    | PDF_TEXT
    | PDF_ITEMS
    | PDF_STRUCTURE
    | PDF_GEOMETRY
    | PDF_RENDER
    | PDF_ANALYSIS
    | PDF_TABLES;

pub(super) unsafe fn run(state: &DocumentState, r: &PdfRequest) -> Fallible<PdfResult> {
    let started = Instant::now();
    if r.outputs & !ALL_OUTPUTS != 0 {
        return Err(Failure::invalid("unknown output flags"));
    }
    let position = position_options(r)?;
    let frame = position.frame;
    let selected = pages(r.pages, state.count)?;
    let md = markdown(&r.markdown)?;
    let render_options = render(&r.render)?;
    let mode = ocr_mode(r.ocr.mode)?;
    ratio(r.ocr.minimum_confidence, "minimum OCR confidence")?;
    ratio(r.ocr.hosted_confidence, "hosted confidence")?;
    if r.ocr.download_policy > PDF_DOWNLOAD_OFFLINE {
        return Err(Failure::invalid("unknown model download policy"));
    }
    if optional_text(r.ocr.model_directory)?.is_some() {
        path(r.ocr.model_directory)?;
    }
    if r.outputs & PDF_RENDER != 0 && !cfg!(feature = "render-pdfium") {
        return Err(Failure::unsupported("rendering was not compiled in"));
    }
    if r.ocr.mode != PDF_OCR_OFF && !cfg!(feature = "ocr") {
        return Err(Failure::unsupported("native OCR was not compiled in"));
    }
    let regions = slice(r.regions.ptr, r.regions.len)?;
    for region in regions {
        page(region.page, state.count)?;
        bounds(region.bounds)?;
        if region.kind > PDF_REGION_GRID {
            return Err(Failure::invalid("unknown region kind"));
        }
    }
    let tables = slice(r.tables.ptr, r.tables.len)?;
    let table_inputs = tables
        .iter()
        .map(|t| table_input(t, state.count))
        .collect::<Fallible<Vec<_>>>()?;
    let external = slice(r.external_ocr.ptr, r.external_ocr.len)?;
    let external_run = external_run(external, state, &selected)?;
    let external_pages = external.iter().map(|p| p.page).collect::<HashSet<_>>();
    let config = detection(r.detection, state.count)?;
    let ocr_on = r.ocr.mode != PDF_OCR_OFF;
    let want_frame_content = r.outputs & (PDF_TEXT | PDF_ITEMS | PDF_GEOMETRY) != 0;
    let want_raw_content = r.outputs & PDF_TABLES != 0;
    let want_pages = r.outputs & (PDF_MARKDOWN | PDF_ANALYSIS) != 0 || !external.is_empty();

    // 1. Classification. Per-page analysis and Markdown come from the
    //    page-oriented passes below, so detection alone is enough here.
    let inspection = pdf_inspector::detect_pdf_type_mem_with_config(&state.bytes, config)?;
    let mut storage = Storage::default();
    let mut view = PdfResultView {
        present: r.outputs | PDF_INSPECTION,
        page_count: state.count,
        pdf_type: match inspection.pdf_type {
            pdf_inspector::PdfType::TextBased => PDF_TYPE_TEXT,
            pdf_inspector::PdfType::Scanned => PDF_TYPE_SCANNED,
            pdf_inspector::PdfType::ImageBased => PDF_TYPE_IMAGE,
            pdf_inspector::PdfType::Mixed => PDF_TYPE_MIXED,
        },
        confidence: inspection.confidence,
        has_encoding_issues: u32::from(inspection.ocr_reasons_by_page.values().any(|r| garbled(r))),
        ocr_recommended: u32::from(inspection.ocr_recommended),
        pages_sampled: inspection.pages_sampled,
        pages_with_text: inspection.pages_with_text,
        title: storage.optional(inspection.title.as_deref()),
        audit: state.audit,
        ..PdfResultView::default()
    };

    // 2. Native OCR replaces the per-page native pass for its pages.
    let local_pages = selected
        .iter()
        .filter(|p| !external_pages.contains(p))
        .copied()
        .collect::<Vec<_>>();
    let ocr = native_ocr(state, r, &md, &render_options, mode, &local_pages)?;
    let mut final_pages: BTreeMap<u32, FusedPageMarkdown> = BTreeMap::new();
    let mut final_table_pages: HashSet<u32> = ocr.tables.iter().copied().collect();
    let ocr_reasons: BTreeMap<u32, Vec<String>> = ocr
        .reasons
        .into_iter()
        .map(|entry| (entry.page, entry.reasons))
        .collect();
    for page in ocr.pages {
        final_pages.insert(page.page_number, page);
    }
    let ocr_covers_all = ocr_on && external.is_empty();

    // 3. Native per-page pass (Markdown, OCR routing, layout flags).
    let mut native = if want_pages && !ocr_covers_all {
        let zero = selected.iter().map(|p| p - 1).collect::<Vec<_>>();
        Some(pdf_inspector::extract_pages_markdown_mem_with_options(
            &state.bytes,
            Some(&zero),
            None,
            &md,
        )?)
    } else {
        None
    };
    if native.is_some() || ocr.ran {
        view.present |= PDF_ANALYSIS;
    }

    // 4. Externally recognized pages go through the core's public fusion.
    if !external.is_empty() {
        let native = native.as_mut().unwrap();
        let (candidates, rest): (Vec<PageMarkdown>, Vec<PageMarkdown>) =
            std::mem::take(&mut native.pages)
                .into_iter()
                .partition(|p| external_pages.contains(&(p.page + 1)));
        native.pages = rest;
        let options = pdf_inspector::vision::OcrFusionOptions::new()
            .markdown(md.clone())
            .hosted_recommendation_confidence(r.ocr.hosted_confidence);
        let fused = pdf_inspector::vision::fuse_ocr_pages(
            &candidates,
            &external_run,
            state.count,
            &options,
        )
        .map_err(|e| Failure::invalid(e.to_string()))?;
        for page in fused.pages {
            if markdown_has_table(&page.markdown) {
                final_table_pages.insert(page.page_number);
            }
            final_pages.insert(page.page_number, page);
        }
    }

    // 5. Positioned content, structure, and rendering. Frame content arrives
    // y-up in the request frame from tested core helpers; raw user-space
    // content stays on the detector path where the pipeline expects it.
    let content = if want_frame_content {
        let set = selected.iter().copied().collect::<HashSet<_>>();
        Some(
            pdf_inspector::extractor::extract_positioned_page_content_mem_in_frame(
                &state.bytes,
                Some(&set),
                position,
            )?,
        )
    } else {
        None
    };
    // TEXT grouping is pinned to the sheet frame so the frame governs geometry,
    // not line breaks. Only parsed when display text is requested.
    let sheet_content = if r.outputs & PDF_TEXT != 0 && frame == PositionFrame::Display {
        let set = selected.iter().copied().collect::<HashSet<_>>();
        Some(
            pdf_inspector::extractor::extract_positioned_page_content_mem_in_frame(
                &state.bytes,
                Some(&set),
                position.frame(PositionFrame::Sheet),
            )?,
        )
    } else {
        None
    };
    let raw_content = if want_raw_content {
        let set = selected.iter().copied().collect::<HashSet<_>>();
        Some(
            pdf_inspector::extractor::extract_positioned_page_content_mem(
                &state.bytes,
                Some(&set),
                position,
            )?,
        )
    } else {
        None
    };
    let doc = if r.outputs & (PDF_STRUCTURE | PDF_TABLES | PDF_ITEMS) != 0 {
        Some(state.document()?)
    } else {
        None
    };
    let elements = if r.outputs & PDF_STRUCTURE != 0 {
        pdf_inspector::extract_structure_elements_mem(&state.bytes, Some(&selected))?
    } else {
        Vec::new()
    };
    #[cfg(feature = "render-pdfium")]
    let rendered = if r.outputs & PDF_RENDER != 0 {
        super::runtime::renderer()?
            .render_pages(&state.bytes, &selected, None, &render_options)
            .map_err(Failure::runtime)?
    } else {
        Vec::new()
    };

    let dest_links = if let (Some(doc), true) = (&doc, r.outputs & PDF_ITEMS != 0) {
        super::links::dest_items(&mut storage, doc, &selected, state, frame)
    } else {
        Vec::new()
    };

    let mut page_views = Vec::new();
    let mut markdown_parts = Vec::new();
    let mut text_parts = Vec::new();
    for (index, number) in selected.iter().enumerate() {
        let frame_info = state.frame_info(*number)?;
        let frame_height = match frame {
            PositionFrame::Sheet => frame_info.sheet_height,
            PositionFrame::Display => frame_info.display_height,
        };
        let info = state.page_info(*number, frame)?;
        #[cfg(feature = "render-pdfium")]
        let (render_x0, render_y1) = (
            frame_info.sheet_x0,
            frame_info.sheet_y0 + frame_info.sheet_height,
        );
        let mut p = PdfPage {
            info,
            ..PdfPage::default()
        };
        let native_page = native
            .as_ref()
            .and_then(|n| n.pages.iter().find(|p| p.page + 1 == *number));
        let reasons: &[String] = ocr_reasons
            .get(number)
            .map(Vec::as_slice)
            .or_else(|| {
                native.as_ref().and_then(|n| {
                    n.ocr_reasons_by_page
                        .iter()
                        .find(|p| p.page == *number)
                        .map(|p| p.reasons.as_slice())
                })
            })
            .or_else(|| {
                inspection
                    .ocr_reasons_by_page
                    .get(number)
                    .map(Vec::as_slice)
            })
            .unwrap_or(&[]);
        p.ocr_reasons = storage.strings(reasons);
        if native_page.is_some_and(|p| p.needs_ocr)
            || ocr.needs.contains(number)
            || inspection.pages_needing_ocr.contains(number)
        {
            p.flags |= PDF_PAGE_NEEDS_OCR;
        }
        if ocr.ran && ocr.needs.contains(number) && !ocr.routed.contains(number) {
            p.flags |= PDF_PAGE_NATIVE_RECOVERED;
        }
        if native
            .as_ref()
            .is_some_and(|n| n.pages_with_tables.contains(number))
            || final_table_pages.contains(number)
        {
            p.flags |= PDF_PAGE_HAS_TABLES;
        }
        if native
            .as_ref()
            .is_some_and(|n| n.pages_with_columns.contains(number))
            || ocr.columns.contains(number)
        {
            p.flags |= PDF_PAGE_HAS_COLUMNS;
        }
        if view.present & PDF_ANALYSIS != 0
            && (garbled(reasons)
                || native_page.is_some_and(|p| {
                    p.ocr_reason.as_deref()
                        == Some(pdf_inspector::OCR_REASON_SUSPECTED_GARBLED_TEXT)
                }))
        {
            view.has_encoding_issues = 1;
            p.flags |= PDF_PAGE_ENCODING_ISSUES;
        }
        let final_page = final_pages.get(number);
        if let Some(f) = final_page {
            p.provenance = provenance(&mut storage, &f.provenance)?;
            // External spans arrive in page points, with no render DPI supplied.
            if external_pages.contains(number) {
                p.provenance.render_dpi = 0.0;
            }
            if f.provenance.ocr_model.is_some() {
                p.flags |= PDF_PAGE_OCR_RAN;
            }
            if f.provenance.hosted_recommended {
                p.flags |= PDF_PAGE_HOSTED_RECOMMENDED;
            }
        }
        let page_md = final_page
            .map(|p| p.markdown.as_str())
            .or_else(|| {
                native_page
                    .filter(|p| !p.needs_ocr)
                    .map(|p| p.markdown.as_str())
            })
            .unwrap_or("");
        if r.outputs & PDF_MARKDOWN != 0 {
            p.markdown = storage.bytes(page_md.as_bytes());
            markdown_parts.push((*number, page_md.to_owned()));
        }
        let page_items = content
            .as_ref()
            .map(|c| {
                c.items
                    .iter()
                    .filter(|i| i.page == *number)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(content) = &content {
            if r.outputs & PDF_ITEMS != 0 {
                let mut items = page_items
                    .iter()
                    .map(|i| storage.item(i, frame_height))
                    .collect::<Vec<_>>();
                items.extend(dest_links.iter().filter(|item| item.page == *number));
                p.items = storage.items(items);
            }
            if r.outputs & PDF_TEXT != 0 {
                // TEXT stays on the sheet frame so the frame governs geometry,
                // not line breaks.
                let text_items = if frame == PositionFrame::Display {
                    sheet_content
                        .as_ref()
                        .map(|sheet| {
                            sheet
                                .items
                                .iter()
                                .filter(|i| i.page == *number)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_else(|| page_items.clone())
                } else {
                    page_items.clone()
                };
                let text = pdf_inspector::extractor::group_into_lines_preserving_all_text(
                    text_items.iter().map(|i| (*i).clone()).collect(),
                )
                .into_iter()
                .map(|mut line| {
                    for item in &mut line.items {
                        item.is_bold = false;
                        item.is_italic = false;
                        item.is_underline = false;
                        item.is_strikeout = false;
                        item.baseline_shift = 0.0;
                    }
                    line.text()
                })
                .collect::<Vec<_>>()
                .join("\n");
                p.text = storage.bytes(text.as_bytes());
                text_parts.push(text);
            }
            if r.outputs & PDF_GEOMETRY != 0 {
                p.rectangles = storage.rectangles(
                    content
                        .rects
                        .iter()
                        .filter(|v| v.page == *number)
                        .map(|v| PdfRectangle {
                            page: *number,
                            bounds: flip_box(v.x, v.y, v.width, v.height, frame_height),
                        })
                        .collect(),
                );
                p.lines = storage.segments(
                    content
                        .lines
                        .iter()
                        .filter(|v| v.page == *number)
                        .map(|v| PdfSegment {
                            page: *number,
                            start: flip_point(v.x1, v.y1, frame_height),
                            end: flip_point(v.x2, v.y2, frame_height),
                        })
                        .collect(),
                );
            }
            if content.gid_pages.contains(number) {
                p.flags |= PDF_PAGE_GID_ENCODED;
            }
            if content.skipped_invisible.contains(number) {
                p.flags |= PDF_PAGE_SKIPPED_INVISIBLE;
            }
            let has_table = p.flags & PDF_PAGE_HAS_TABLES != 0;
            // Both detectors filter by page, so hand them only this page's items.
            let page_owned = page_items
                .iter()
                .map(|i| (*i).clone())
                .collect::<Vec<pdf_inspector::TextItem>>();
            let (columns, newspaper) =
                pdf_inspector::extractor::page_column_layout(&page_owned, *number, has_table);
            if columns.len() < 2 {
                p.reading_order = PDF_READING_SINGLE;
            } else {
                p.flags |= PDF_PAGE_HAS_COLUMNS;
                p.reading_order = if newspaper {
                    PDF_READING_NEWSPAPER
                } else {
                    PDF_READING_TABULAR
                };
            }
            p.columns = storage.intervals(
                columns
                    .into_iter()
                    .map(|(x0, x1)| PdfInterval {
                        x0: x0.min(x1),
                        x1: x0.max(x1),
                    })
                    .collect(),
            );
            p.charts = storage.boxes(
                pdf_inspector::tables::detect_chart_regions(&page_owned, &content.rects, *number)
                    .into_iter()
                    .map(|(x, y, w, h)| flip_box(x, y, w, h, frame_height))
                    .collect(),
            );
            p.image_regions = storage.boxes(
                page_items
                    .iter()
                    .filter_map(|item| image_region_box(item, frame_height))
                    .collect(),
            );
        }
        if r.outputs & (PDF_ITEMS | PDF_GEOMETRY | PDF_TEXT | PDF_MARKDOWN | PDF_ANALYSIS) != 0 {
            let item_text = page_items
                .iter()
                .filter(|i| matches!(i.item_type, pdf_inspector::types::ItemType::Text))
                .map(|i| i.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let quality_text = if item_text.trim().is_empty() {
                page_md
            } else {
                item_text.as_str()
            };
            p.quality =
                super::output::quality_view(pdf_inspector::text_quality_metrics(quality_text));
        }
        if r.outputs & PDF_STRUCTURE != 0 {
            let elements = elements
                .iter()
                .filter(|e| e.page == *number)
                .map(|e| PdfStructureElement {
                    page: e.page,
                    mcid: e.mcid,
                    role: storage.bytes(e.role.as_bytes()),
                })
                .collect();
            p.structure = storage.structure_elements(elements);
        }
        #[cfg(feature = "render-pdfium")]
        if r.outputs & PDF_RENDER != 0 {
            p.image = image_view(&mut storage, &rendered[index], render_x0, render_y1)?;
        }
        #[cfg(not(feature = "render-pdfium"))]
        let _ = index;
        page_views.push(p);
    }
    if r.outputs & PDF_MARKDOWN != 0 {
        let document_markdown = match ocr.markdown {
            Some(markdown) if external.is_empty() => markdown,
            _ => assemble(&markdown_parts, md.include_page_numbers),
        };
        view.markdown = storage.bytes(document_markdown.into_bytes());
    }
    if r.outputs & PDF_TEXT != 0 {
        view.text = storage.bytes(text_parts.join("\n\n").into_bytes());
    }
    let region_views = region_queries(&mut storage, state, regions, position)?;
    view.regions = storage.regions(region_views);
    let mut table_views = match (&raw_content, r.outputs & PDF_TABLES != 0) {
        (Some(content), true) => super::tables::detect(
            &mut storage,
            content,
            doc.as_ref(),
            &selected,
            state,
            md.clone(),
            frame,
        ),
        _ => Vec::new(),
    };
    if !tables.is_empty() {
        view.present |= PDF_TABLES;
        table_views.extend(hinted_tables(&mut storage, state, tables, table_inputs)?);
    }
    for table in &table_views {
        if table.flags & PDF_TABLE_FROM_HINT == 0 {
            if let Some(page) = page_views
                .iter_mut()
                .find(|page| page.info.page == table.page)
            {
                page.flags |= PDF_PAGE_HAS_TABLES;
            }
        }
    }
    view.pages = storage.pages(page_views);
    if view.present & PDF_TABLES != 0 {
        view.tables = storage.tables(table_views);
    }
    if let (Some(doc), true) = (&doc, r.outputs & PDF_STRUCTURE != 0) {
        view.structure_nodes = super::semantic::nodes(&mut storage, doc, &selected);
    }
    view.processing_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    Ok(PdfResult {
        view,
        _storage: storage,
    })
}
/// Native OCR results for the locally processed pages.
#[derive(Default)]
struct OcrOutcome {
    ran: bool,
    pages: Vec<FusedPageMarkdown>,
    tables: Vec<u32>,
    columns: Vec<u32>,
    needs: Vec<u32>,
    routed: Vec<u32>,
    reasons: Vec<pdf_inspector::PageOcrReasons>,
    markdown: Option<String>,
}
#[cfg(feature = "ocr")]
unsafe fn native_ocr(
    state: &DocumentState,
    r: &PdfRequest,
    md: &pdf_inspector::MarkdownOptions,
    render_options: &pdf_inspector::vision::RenderOptions,
    mode: pdf_inspector::vision::OcrMode,
    local_pages: &[u32],
) -> Fallible<OcrOutcome> {
    if mode == pdf_inspector::vision::OcrMode::Off || local_pages.is_empty() {
        return Ok(OcrOutcome::default());
    }
    let options = pdf_inspector::vision::OcrPdfOptions::new()
        .page_numbers(local_pages.iter().copied())
        .markdown(md.clone())
        .render(render_options.clone())
        .ocr(pdf_inspector::vision::OcrOptions {
            mode,
            minimum_confidence: r.ocr.minimum_confidence,
            model_directory: optional_text(r.ocr.model_directory)?.map(Into::into),
            model_downloads: if r.ocr.download_policy == PDF_DOWNLOAD_OFFLINE {
                pdf_inspector::vision::ModelDownloadPolicy::Offline
            } else {
                pdf_inspector::vision::ModelDownloadPolicy::IfMissing
            },
        })
        .hosted_recommendation_confidence(r.ocr.hosted_confidence);
    let result = pdf_inspector::vision::process_pdf_with_ocr_mem(&state.bytes, options)
        .map_err(Failure::runtime)?;
    Ok(OcrOutcome {
        ran: true,
        pages: result.pages,
        tables: result.pages_with_tables,
        columns: result.pages_with_columns,
        needs: result.pages_recommended_for_ocr,
        routed: result.pages_routed_to_ocr,
        reasons: result.ocr_reasons_by_page,
        markdown: Some(result.markdown),
    })
}
#[cfg(not(feature = "ocr"))]
unsafe fn native_ocr(
    _state: &DocumentState,
    _r: &PdfRequest,
    _md: &pdf_inspector::MarkdownOptions,
    _render_options: &pdf_inspector::vision::RenderOptions,
    _mode: pdf_inspector::vision::OcrMode,
    _local_pages: &[u32],
) -> Fallible<OcrOutcome> {
    Ok(OcrOutcome::default())
}
fn image_region_box(item: &pdf_inspector::TextItem, frame_height: f32) -> Option<PdfBox> {
    pdf_inspector::significant_image_region(item)
        .map(|r| flip_box(r.x, r.y, r.width, r.height, frame_height))
}
fn garbled(reasons: &[String]) -> bool {
    reasons
        .iter()
        .any(|reason| reason == pdf_inspector::OCR_REASON_SUSPECTED_GARBLED_TEXT)
}
unsafe fn detection(d: PdfDetectionOptions, count: u32) -> Fallible<DetectionConfig> {
    ratio(d.text_page_ratio, "text-page ratio")?;
    let strategy = match d.strategy {
        PDF_SCAN_SAMPLE if d.sample_size > 0 => ScanStrategy::Sample(d.sample_size),
        PDF_SCAN_FULL => ScanStrategy::Full,
        PDF_SCAN_EARLY_EXIT => ScanStrategy::EarlyExit,
        PDF_SCAN_PAGES => ScanStrategy::Pages(pages(d.pages, count)?),
        _ => {
            return Err(Failure::invalid(
                "invalid detection strategy or sample size",
            ))
        }
    };
    Ok(DetectionConfig {
        strategy,
        min_text_ops_per_page: d.min_text_ops,
        text_page_ratio_threshold: d.text_page_ratio,
    })
}
fn assemble(parts: &[(u32, String)], numbers: bool) -> String {
    parts
        .iter()
        .map(|(p, text)| {
            if numbers {
                format!("<!-- Page {p} -->\n\n{text}")
            } else {
                text.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
/// A GFM table separator row marks a table in final page Markdown.
fn markdown_has_table(markdown: &str) -> bool {
    markdown.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with('|')
            && trimmed.ends_with('|')
            && trimmed
                .split('|')
                .filter(|cell| !cell.is_empty())
                .all(|cell| cell.chars().all(|ch| ch == '-'))
    })
}
fn provenance(
    s: &mut Storage,
    p: &pdf_inspector::vision::PageProvenance,
) -> Fallible<PdfProvenance> {
    Ok(PdfProvenance {
        source: match p.source {
            PageContentSource::Native => PDF_CONTENT_NATIVE,
            PageContentSource::Ocr => PDF_CONTENT_OCR,
            PageContentSource::Fused => PDF_CONTENT_FUSED,
            _ => return Err(Failure::unsupported("unmapped core page content source")),
        },
        flags: u32::from(p.ocr_confidence.is_some()) * PDF_HAS_CONFIDENCE,
        confidence: p.ocr_confidence.unwrap_or(0.0),
        render_dpi: p.render_dpi.unwrap_or(0.0),
        render_ms: p.timings.render_ms,
        ocr_ms: p.timings.ocr_ms,
        assembly_ms: p.timings.assembly_ms,
        model: s.optional(p.ocr_model.as_ref().map(|m| m.name.as_str())),
        model_revision: s.optional(p.ocr_model.as_ref().map(|m| m.revision.as_str())),
        warnings: s.strings(&p.warnings),
    })
}

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
unsafe fn table_input(t: &PdfTableInput, count: u32) -> Fallible<pdf_inspector::TsrTableInput> {
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
/// Resolve every hinted table with two batched core calls at most.
fn hinted_tables(
    storage: &mut Storage,
    state: &DocumentState,
    tables: &[PdfTableInput],
    inputs: Vec<pdf_inspector::TsrTableInput>,
) -> Fallible<Vec<PdfTable>> {
    let cells = pdf_inspector::extract_tables_with_structure_cells_mem(&state.bytes, &inputs)?;
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
    let mut auto =
        pdf_inspector::extract_tables_with_structure_auto_mem(&state.bytes, &auto_inputs)?
            .into_iter();
    let mut views = Vec::with_capacity(tables.len());
    for (input_index, (input, cells)) in tables.iter().zip(cells).enumerate() {
        let (markdown, fallback) = if input.mode == PDF_TSR_AUTO {
            let r = auto.next().unwrap();
            (r.markdown, r.fallback_reason)
        } else {
            (pdf_inspector::tables::cells_to_markdown(&cells), None)
        };
        let cell_views = if fallback.is_none() {
            cells
                .iter()
                .map(|c| PdfCell {
                    row: c.row,
                    column: c.col,
                    row_span: c.rowspan,
                    column_span: c.colspan,
                    flags: (u32::from(c.is_header) * PDF_CELL_HEADER)
                        | PDF_CELL_HAS_BOUNDS
                        | PDF_CELL_SPAN_KNOWN,
                    bounds: page_box(c.page_pt_bbox),
                    text: storage.bytes(c.text.as_bytes()),
                })
                .collect()
        } else {
            Vec::new()
        };
        views.push(PdfTable {
            page: input.page,
            flags: PDF_TABLE_FROM_HINT | PDF_TABLE_HAS_BOUNDS,
            input_index,
            bounds: input.bounds,
            markdown: storage.bytes(markdown.into_bytes()),
            fallback_reason: storage.optional(fallback.as_deref()),
            cells: storage.cells(cell_views),
            ..PdfTable::default()
        });
    }
    Ok(views)
}
/// Run text and table region queries as one batched core call per kind and
/// vector-grid queries individually, preserving descriptor order.
///
/// `position` is the caller-visible frame of `regions` plus bold-from-weight
/// knobs: display rects are read in that frame, while views echo the
/// caller's original bounds.
fn region_queries(
    s: &mut Storage,
    state: &DocumentState,
    regions: &[PdfRegionInput],
    position: PositionOptions,
) -> Fallible<Vec<PdfRegion>> {
    let mut views = regions
        .iter()
        .map(|r| PdfRegion {
            page: r.page,
            kind: r.kind,
            bounds: r.bounds,
            ..PdfRegion::default()
        })
        .collect::<Vec<_>>();
    for kind in [PDF_REGION_TEXT, PDF_REGION_TABLE] {
        let indices = regions
            .iter()
            .enumerate()
            .filter(|(_, r)| r.kind == kind)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if indices.is_empty() {
            continue;
        }
        let request = indices
            .iter()
            .map(|&i| {
                let rect = bounds(regions[i].bounds)?;
                Ok((regions[i].page - 1, vec![rect]))
            })
            .collect::<Fallible<Vec<_>>>()?;
        let results = if kind == PDF_REGION_TEXT {
            pdf_inspector::extract_text_in_regions_mem_with_options(
                &state.bytes,
                &request,
                position,
            )?
        } else {
            pdf_inspector::extract_tables_in_regions_mem_with_options(
                &state.bytes,
                &request,
                position,
            )?
        };
        for (&i, mut result) in indices.iter().zip(results) {
            let Some(region) = result.regions.pop() else {
                continue;
            };
            views[i].flags = u32::from(region.needs_ocr) * PDF_REGION_NEEDS_OCR;
            views[i].text = s.bytes(region.text.into_bytes());
            views[i].ocr_reason = s.optional(region.ocr_reason.as_deref());
        }
    }
    for (i, r) in regions.iter().enumerate() {
        if r.kind != PDF_REGION_GRID {
            continue;
        }
        let bbox = bounds(r.bounds)?;
        if let Some(grid) =
            pdf_inspector::detect_vector_grid_in_region_mem(&state.bytes, r.page - 1, bbox, 72.0)?
        {
            views[i].flags |= PDF_REGION_GRID_FOUND;
            views[i].tokens = s.strings(grid.structure_tokens);
            views[i].cells = s.boxes(
                grid.cell_bboxes
                    .into_iter()
                    .map(|b| PdfBox {
                        x0: b[0] + bbox[0],
                        y0: b[1] + bbox[1],
                        x1: b[2] + bbox[0],
                        y1: b[3] + bbox[1],
                    })
                    .collect(),
            );
        }
    }
    Ok(views)
}

fn validate_quad(q: PdfQuad) -> Fallible<()> {
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
/// A pixel-free page whose bitmap frame equals the page-point frame at 72 dpi,
/// so externally supplied page-space quadrilaterals enter the core's fusion
/// unchanged. The zeroed buffer is never read.
fn phantom_page(info: PdfPageInfo) -> Fallible<RenderedPage> {
    let width = info.width.ceil().max(1.0) as u32;
    let height = info.height.ceil().max(1.0) as u32;
    let top = f64::from(info.height);
    let transform = PageTransform::from_corners(
        width,
        height,
        (0.0, top),
        (f64::from(width), top),
        (0.0, top - f64::from(height)),
    )
    .ok_or_else(|| Failure::invalid("page dimensions are degenerate"))?;
    RenderedPage::new(
        info.page,
        info.width,
        info.height,
        width,
        height,
        width as usize,
        RenderPixelFormat::Gray8,
        vec![0; width as usize * height as usize],
        transform,
    )
    .map_err(|e| Failure::invalid(e.to_string()))
}
unsafe fn external_run(
    external: &[PdfOcrPageInput],
    state: &DocumentState,
    selected: &[u32],
) -> Fallible<OcrRun> {
    let mut pages = Vec::new();
    let mut seen = BTreeSet::new();
    let mut time = 0u64;
    for e in external {
        page(e.page, state.count)?;
        if !selected.contains(&e.page) || !seen.insert(e.page) {
            return Err(Failure::invalid(
                "external OCR pages must be unique and selected",
            ));
        }
        if e.flags & !PDF_HAS_CONFIDENCE != 0 {
            return Err(Failure::invalid("unknown OCR page flags"));
        }
        let confidence = if e.flags & PDF_HAS_CONFIDENCE != 0 {
            Some(ratio(e.confidence, "OCR page confidence")?)
        } else {
            None
        };
        let mut spans = Vec::new();
        for s in slice(e.spans.ptr, e.spans.len)? {
            validate_quad(s.polygon)?;
            ratio(s.confidence, "OCR span confidence")?;
            if s.flags & !PDF_HAS_ORIENTATION != 0 {
                return Err(Failure::invalid("unknown OCR span flags"));
            }
            let angle = if s.flags & PDF_HAS_ORIENTATION != 0 {
                Some(finite(s.orientation, "OCR orientation")?)
            } else {
                None
            };
            spans.push(OcrSpan {
                text: text(s.text)?.into(),
                polygon: ImageQuad::new(s.polygon.points.map(|p| ImagePoint::new(p.x, p.y))),
                confidence: s.confidence,
                orientation_degrees: angle,
            });
        }
        let warnings = slice(e.warnings.ptr, e.warnings.len)?
            .iter()
            .map(|s| text(*s).map(str::to_owned))
            .collect::<Fallible<Vec<_>>>()?;
        time = time.saturating_add(e.processing_ms);
        pages.push(RoutedOcrPage {
            rendered: phantom_page(state.sheet_info(e.page)?)?,
            ocr: OcrPage {
                page_number: e.page,
                spans,
                mean_confidence: confidence,
                model: ModelIdentity::new(text(e.model)?, text(e.model_revision)?),
                processing_time_ms: e.processing_ms,
                warnings,
            },
        });
    }
    Ok(OcrRun {
        pages,
        render_time_ms: 0,
        ocr_time_ms: time,
    })
}
#[cfg(feature = "render-pdfium")]
fn pixel_transform(t: PageTransform, sheet_x0: f32, sheet_y1: f32) -> PdfTransform {
    let p = t.pixel_to_page(0.0, 0.0);
    let width = f64::from(t.pixel_width());
    let height = f64::from(t.pixel_height());
    let x = t.pixel_to_page(width, 0.0);
    let y = t.pixel_to_page(0.0, height);
    PdfTransform {
        a: f64::from(x.x - p.x) / width,
        b: f64::from(p.y - x.y) / width,
        c: f64::from(y.x - p.x) / height,
        d: f64::from(p.y - y.y) / height,
        e: f64::from(p.x) - f64::from(sheet_x0),
        f: f64::from(sheet_y1) - f64::from(p.y),
    }
}
#[cfg(feature = "render-pdfium")]
fn image_view(
    s: &mut Storage,
    r: &RenderedPage,
    sheet_x0: f32,
    sheet_y1: f32,
) -> Fallible<PdfImage> {
    let transform = pixel_transform(r.transform(), sheet_x0, sheet_y1);
    Ok(PdfImage {
        width: r.width(),
        height: r.height(),
        stride: r.stride(),
        format: match r.format() {
            RenderPixelFormat::Rgb8 => PDF_RGB8,
            RenderPixelFormat::Rgba8 => PDF_RGBA8,
            RenderPixelFormat::Gray8 => PDF_GRAY8,
            _ => return Err(Failure::unsupported("unmapped core pixel format")),
        },
        pixels: s.bytes(r.pixels()),
        pixel_to_page: transform,
        page_to_pixel: transform.inverse()?,
    })
}

pub(super) unsafe fn compose(c: &PdfComposeInput) -> Fallible<PdfResult> {
    let options = markdown(&c.markdown)?;
    let mut storage = Storage::default();
    let (markdown, page_count) = match c.kind {
        PDF_COMPOSE_TEXT => (pdf_inspector::to_markdown(text(c.text)?, options), 0),
        PDF_COMPOSE_ITEMS => {
            let pages = slice(c.pages.ptr, c.pages.len)?;
            let mut by_page = BTreeMap::new();
            for p in pages {
                if p.page == 0
                    || p.width <= 0.0
                    || p.height <= 0.0
                    || !p.width.is_finite()
                    || !p.height.is_finite()
                    || p.rotation >= 360
                    || !p.rotation.is_multiple_of(90)
                    || by_page.insert(p.page, *p).is_some()
                {
                    return Err(Failure::invalid("invalid or duplicate composition page"));
                }
            }
            let count = by_page.keys().next_back().copied().unwrap_or(0);
            let mut items = Vec::new();
            for i in slice(c.items.ptr, c.items.len)? {
                let p = by_page
                    .get(&i.page)
                    .ok_or_else(|| Failure::invalid("item page has no dimensions"))?;
                if i.flags
                    & !(PDF_BOLD
                        | PDF_ITALIC
                        | PDF_UNDERLINE
                        | PDF_STRIKEOUT
                        | PDF_HAS_MCID
                        | PDF_ADVANCE_KNOWN
                        | PDF_LEGACY_SYMBOL_REWRITE)
                    != 0
                {
                    return Err(Failure::invalid("unknown item flags"));
                }
                for f in [
                    i.bounds.x0,
                    i.bounds.y0,
                    i.bounds.x1,
                    i.bounds.y1,
                    i.font_size,
                    i.rotation,
                    i.baseline_shift,
                ] {
                    finite(f, "item geometry")?;
                }
                if i.bounds.x1 < i.bounds.x0
                    || i.bounds.y1 < i.bounds.y0
                    || i.font_size < 0.0
                    || !(0.0..360.0).contains(&i.rotation)
                {
                    return Err(Failure::invalid("invalid item geometry"));
                }
                let kind = match i.kind {
                    PDF_ITEM_TEXT => pdf_inspector::types::ItemType::Text,
                    PDF_ITEM_IMAGE => pdf_inspector::types::ItemType::Image,
                    PDF_ITEM_LINK => {
                        let uri = text(i.link)?;
                        if uri.is_empty() && i.dest_page == 0 {
                            return Err(Failure::invalid("link item has no URI or dest_page"));
                        }
                        pdf_inspector::types::ItemType::Link(uri.into())
                    }
                    PDF_ITEM_FORM_FIELD => pdf_inspector::types::ItemType::FormField,
                    _ => return Err(Failure::invalid("invalid item kind")),
                };
                items.push(pdf_inspector::TextItem {
                    text: text(i.text)?.into(),
                    x: i.bounds.x0,
                    y: p.height - i.bounds.y1,
                    width: i.bounds.x1 - i.bounds.x0,
                    height: i.bounds.y1 - i.bounds.y0,
                    rotation: (-i.rotation).rem_euclid(360.0),
                    advance_known: i.flags & PDF_ADVANCE_KNOWN != 0,
                    font: text(i.font)?.into(),
                    font_tag: text(i.font_tag)?.into(),
                    legacy_symbol_rewrite: i.flags & PDF_LEGACY_SYMBOL_REWRITE != 0,
                    font_size: i.font_size,
                    page: i.page,
                    is_bold: i.flags & PDF_BOLD != 0,
                    is_italic: i.flags & PDF_ITALIC != 0,
                    font_weight: super::output::parse_font_weight(i.font_weight)?,
                    bold_source: super::output::parse_bold_source(i.bold_source)?,
                    fixed_pitch: super::output::parse_fixed_pitch(i.fixed_pitch)?,
                    is_underline: i.flags & PDF_UNDERLINE != 0,
                    is_strikeout: i.flags & PDF_STRIKEOUT != 0,
                    item_type: kind,
                    mcid: (i.flags & PDF_HAS_MCID != 0).then_some(i.mcid),
                    baseline_shift: i.baseline_shift,
                });
            }
            let mut rects = Vec::new();
            for r in slice(c.rectangles.ptr, c.rectangles.len)? {
                let p = by_page
                    .get(&r.page)
                    .ok_or_else(|| Failure::invalid("rectangle page has no dimensions"))?;
                bounds(r.bounds)?;
                rects.push(pdf_inspector::PdfRect {
                    page: r.page,
                    x: r.bounds.x0,
                    y: p.height - r.bounds.y1,
                    width: r.bounds.x1 - r.bounds.x0,
                    height: r.bounds.y1 - r.bounds.y0,
                });
            }
            let mut lines = Vec::new();
            for l in slice(c.lines.ptr, c.lines.len)? {
                let p = by_page
                    .get(&l.page)
                    .ok_or_else(|| Failure::invalid("line page has no dimensions"))?;
                for f in [l.start.x, l.start.y, l.end.x, l.end.y] {
                    finite(f, "line geometry")?;
                }
                lines.push(pdf_inspector::PdfLine {
                    page: l.page,
                    x1: l.start.x,
                    y1: p.height - l.start.y,
                    x2: l.end.x,
                    y2: p.height - l.end.y,
                });
            }
            let mut roles = std::collections::HashMap::<
                u32,
                std::collections::HashMap<i64, pdf_inspector::structure_tree::StructRole>,
            >::new();
            for e in slice(c.structure.ptr, c.structure.len)? {
                if !by_page.contains_key(&e.page) {
                    return Err(Failure::invalid("structure page has no dimensions"));
                }
                let role = pdf_inspector::structure_tree::StructRole::from_name(text(e.role)?);
                if roles
                    .entry(e.page)
                    .or_default()
                    .insert(e.mcid, role)
                    .is_some()
                {
                    return Err(Failure::invalid("duplicate structure reference"));
                }
            }
            let value = pdf_inspector::markdown::to_markdown_from_items_with_rects_and_lines(
                items,
                options,
                &rects,
                &lines,
                pdf_inspector::markdown::MarkdownDocumentContext {
                    page_thresholds: &std::collections::HashMap::new(),
                    struct_roles: Some(&roles),
                    struct_tables: &[],
                    page_count: count,
                    prefiltered_page_number_pages: None,
                    prefiltered_page_number_mask: None,
                    precomputed_chart_regions: None,
                },
            );
            (value, count)
        }
        _ => return Err(Failure::invalid("unknown composition kind")),
    };
    Ok(PdfResult {
        view: PdfResultView {
            present: PDF_MARKDOWN,
            page_count,
            markdown: storage.bytes(markdown.into_bytes()),
            ..PdfResultView::default()
        },
        _storage: storage,
    })
}
