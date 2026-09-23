use super::content::PageContent;
use super::frames::PageFrame;
use super::input::*;
use super::output::{flip_box, flip_point, narrow, orientation};
use super::*;
use lopdf::Document;

use lopdf::ObjectId;
use pdf_inspector::detector::PdfTypeResult;
use pdf_inspector::structure_tree::StructTree;
use pdf_inspector::vision::{
    FusedPageMarkdown, ImagePoint, ImageQuad, ModelIdentity, OcrPage, OcrRun, OcrSpan,
    PageContentSource, PageTransform, RenderPixelFormat, RenderedPage, RoutedOcrPage,
};
use pdf_inspector::{
    DetectionConfig, PageMarkdown, PageRotation, PositionFrame, PositionOptions, ScanStrategy,
};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::OnceLock;
use std::time::Instant;

/// The loaded document (decrypted once when a password opened it), its
/// serialized bytes, and per-page frames cached at open. Everything the core
/// reads from a `Document` runs against `doc`; the core's byte-oriented
/// entry points reparse `bytes`, re-applying the loader's repairs each time,
/// so only decryption has to be written back. Request-independent work
/// (default inspection, the structure tree) is computed once, lazily.
pub(super) struct DocumentState {
    bytes: Vec<u8>,
    doc: Document,
    /// 1-indexed page numbers to page objects.
    pages: BTreeMap<u32, ObjectId>,
    frames: Vec<PageFrame>,
    pub(super) info: PdfDocumentInfo,
    /// Backs `info.pages`.
    _storage: Storage,
    default_inspection: OnceLock<PdfTypeResult>,
    structure: OnceLock<Option<StructTree>>,
}
/// Executions share one document across threads: the parsed document and
/// the lazily filled caches must be shareable. (`info` and its arena are
/// written at open and only read afterwards.)
const _: () = {
    const fn shared<T: Send + Sync>() {}
    shared::<(
        Document,
        OnceLock<PdfTypeResult>,
        OnceLock<Option<StructTree>>,
    )>()
};
/// Inspection for one execution: the document's cached default, or a fresh
/// run for a non-default detection configuration.
enum Inspection<'a> {
    Cached(&'a PdfTypeResult),
    Fresh(Box<PdfTypeResult>),
}
impl std::ops::Deref for Inspection<'_> {
    type Target = PdfTypeResult;
    fn deref(&self) -> &PdfTypeResult {
        match self {
            Inspection::Cached(r) => r,
            Inspection::Fresh(r) => r,
        }
    }
}
impl DocumentState {
    pub(super) fn open(bytes: Vec<u8>, password: Option<&str>) -> Fallible<Self> {
        let leading_bytes = pdf_inspector::pdf_header_offset(&bytes).unwrap_or(0);
        let (mut doc, _, repairs) =
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
        if leading_bytes > 0 {
            flags |= PDF_LOAD_LEADING_BYTES;
        }
        if repairs.widened_form_bboxes > 0 {
            flags |= PDF_LOAD_WIDENED_FORM_BBOX;
        }
        if repairs.saturated_bbox_numerals > 0 {
            flags |= PDF_LOAD_SATURATED_BBOX;
        }
        let (bytes, doc) = if encrypted {
            doc.encryption_state = None;
            doc.trailer.remove(b"Encrypt");
            let mut decrypted = Vec::new();
            doc.save_to(&mut decrypted).map_err(|e| {
                Failure::parse(format!("could not serialize decrypted document: {e}"))
            })?;
            // Keep the document as later byte parses will see it.
            let reloaded = pdf_inspector::load_document_from_mem_with_password(&decrypted, None)?.0;
            (decrypted, reloaded)
        } else {
            (bytes, doc)
        };
        let pages = doc.get_pages();
        let frames = PageFrame::all(&doc);
        let storage = Storage::default();
        let doc_info = pdf_inspector::detector::read_document_info(&doc);
        let info = PdfDocumentInfo {
            page_count: narrow(frames.len()),
            audit: PdfLoadAudit {
                flags,
                leading_bytes: narrow(leading_bytes),
                widened_form_bboxes: narrow(repairs.widened_form_bboxes),
                saturated_bbox_numerals: narrow(repairs.saturated_bbox_numerals),
            },
            metadata: storage.metadata(&doc_info),
            pages: storage.slice(frames.iter().map(|f| f.info(PositionFrame::Sheet))),
        };
        Ok(Self {
            bytes,
            doc,
            pages,
            frames,
            info,
            _storage: storage,
            default_inspection: OnceLock::new(),
            structure: OnceLock::new(),
        })
    }
    pub(super) fn count(&self) -> u32 {
        self.info.page_count
    }
    /// Detector inspection under `options`. The default configuration is
    /// run once per document; any other configuration runs per execution.
    fn inspection(
        &self,
        options: PdfDetectionOptions,
        config: DetectionConfig,
    ) -> Fallible<Inspection<'_>> {
        if !is_default_detection(options) {
            let fresh = pdf_inspector::detect_pdf_type_mem_with_config(&self.bytes, config)?;
            return Ok(Inspection::Fresh(Box::new(fresh)));
        }
        if let Some(cached) = self.default_inspection.get() {
            return Ok(Inspection::Cached(cached));
        }
        let fresh = pdf_inspector::detect_pdf_type_mem_with_config(&self.bytes, config)?;
        Ok(Inspection::Cached(
            self.default_inspection.get_or_init(|| fresh),
        ))
    }
    /// The tagged-PDF structure tree, parsed once; `None` for untagged files.
    fn structure(&self) -> Option<&StructTree> {
        self.structure
            .get_or_init(|| StructTree::from_doc(&self.doc))
            .as_ref()
    }
    pub(super) fn frame(&self, page: u32) -> Fallible<&PageFrame> {
        page.checked_sub(1)
            .and_then(|i| self.frames.get(i as usize))
            .ok_or_else(|| Failure::invalid(format!("page {page} is outside 1..={}", self.count())))
    }
}

const ALL_OUTPUTS: u32 = PDF_OUT_INSPECTION
    | PDF_OUT_MARKDOWN
    | PDF_OUT_TEXT
    | PDF_OUT_ITEMS
    | PDF_OUT_STRUCTURE
    | PDF_OUT_GEOMETRY
    | PDF_OUT_RENDER
    | PDF_OUT_ANALYSIS;

pub(super) unsafe fn run(state: &DocumentState, r: &PdfRequest) -> Fallible<ResultOwner> {
    let started = Instant::now();
    reserved(r.outputs, ALL_OUTPUTS, "output flags")?;
    let position = position_options(r)?;
    let extraction = text_extraction_options(r)?;
    let frame = position.frame;
    let selected = pages(r.pages, state.count())?;
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
    if r.outputs & PDF_OUT_RENDER != 0 && !cfg!(feature = "render-pdfium") {
        return Err(Failure::unsupported("rendering was not compiled in"));
    }
    if r.ocr.mode != PDF_OCR_OFF && !cfg!(feature = "ocr") {
        return Err(Failure::unsupported("native OCR was not compiled in"));
    }
    let regions = slice(r.regions.ptr, r.regions.len)?;
    for region in regions {
        page(region.page, state.count())?;
        bounds(region.bounds)?;
        if region.kind > PDF_REGION_GRID {
            return Err(Failure::invalid("unknown region kind"));
        }
    }
    let tables = slice(r.tables.ptr, r.tables.len)?;
    let table_inputs = tables
        .iter()
        .map(|t| super::tables::table_input(t, state.count()))
        .collect::<Fallible<Vec<_>>>()?;
    let external = slice(r.external_ocr.ptr, r.external_ocr.len)?;
    let external_run = external_run(external, state, &selected)?;
    let external_pages = external.iter().map(|p| p.page).collect::<HashSet<_>>();
    let config = detection(r.detection, state.count())?;
    let ocr_on = r.ocr.mode != PDF_OCR_OFF;
    let want_content = r.outputs & (PDF_OUT_TEXT | PDF_OUT_ITEMS | PDF_OUT_GEOMETRY) != 0;
    let want_pages = r.outputs & (PDF_OUT_MARKDOWN | PDF_OUT_ANALYSIS) != 0 || !external.is_empty();

    // 1. Classification. Per-page analysis and Markdown come from the
    //    page-oriented passes below, so detection alone is enough here.
    let inspection = state.inspection(r.detection, config)?;
    let storage = Storage::default();
    let mut view = PdfResult {
        present: r.outputs | PDF_OUT_INSPECTION,
        page_count: state.count(),
        pdf_type: match inspection.pdf_type {
            pdf_inspector::PdfType::TextBased => PDF_TYPE_TEXT,
            pdf_inspector::PdfType::Scanned => PDF_TYPE_SCANNED,
            pdf_inspector::PdfType::ImageBased => PDF_TYPE_IMAGE,
            pdf_inspector::PdfType::Mixed => PDF_TYPE_MIXED,
        },
        confidence: inspection.confidence,
        flags: (u32::from(inspection.ocr_reasons_by_page.values().any(|r| garbled(r)))
            * PDF_DOC_ENCODING_ISSUES)
            | (u32::from(inspection.ocr_recommended) * PDF_DOC_OCR_RECOMMENDED),
        pages_sampled: inspection.pages_sampled,
        pages_with_text: inspection.pages_with_text,
        metadata: storage.metadata_from_inspection(&inspection),
        ..PdfResult::default()
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
        view.present |= PDF_OUT_ANALYSIS;
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
            state.count(),
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

    // 5. Positioned content, structure, and rendering. One parse of the
    // selected pages in the sheet frame; TEXT lines are grouped there so the
    // frame governs geometry, not line breaks, before geometry moves to the
    // display frame when requested.
    let doc = &state.doc;
    let mut content: Option<PageContent> = None;
    let mut sheet_text: BTreeMap<u32, String> = BTreeMap::new();
    if want_content {
        let set = selected.iter().copied().collect::<HashSet<_>>();
        let mut parsed = super::content::parse(doc, &set, extraction)?;
        if r.outputs & PDF_OUT_TEXT != 0 {
            for number in &selected {
                sheet_text.insert(*number, plain_text(&parsed.items, *number));
            }
        }
        if frame == PositionFrame::Display {
            parsed.move_to_display(doc, &state.frames);
        }
        view.cmap_gaps = storage.slice(parsed.cmap_gaps.iter().map(|gap| PdfCMapGap {
            font: storage.bytes(&gap.font),
            codes: gap.codes,
            interpolated: gap.interpolated,
            unmapped: gap.unmapped,
        }));
        if parsed.cmap_gaps.iter().any(|gap| gap.unmapped > 0) {
            view.flags |= PDF_DOC_ENCODING_ISSUES;
        }
        content = Some(parsed);
    }
    // Marked-content roles of the selected pages, sorted by (page, mcid).
    let elements: Vec<PdfStructureElement> = match (r.outputs & PDF_OUT_STRUCTURE != 0)
        .then(|| state.structure())
        .flatten()
    {
        Some(tree) => {
            let mut elements = tree
                .mcid_to_roles(&state.pages)
                .into_iter()
                .filter(|(page, _)| selected.contains(page))
                .flat_map(|(page, roles)| {
                    let storage = &storage;
                    roles
                        .into_iter()
                        .map(move |(mcid, role)| PdfStructureElement {
                            page,
                            mcid,
                            role: storage.bytes(role.name()),
                        })
                })
                .collect::<Vec<_>>();
            elements.sort_by_key(|e| (e.page, e.mcid));
            elements
        }
        None => Vec::new(),
    };
    #[cfg(feature = "render-pdfium")]
    let mut rendered = if r.outputs & PDF_OUT_RENDER != 0 {
        super::runtime::renderer()?
            .render_pages(&state.bytes, &selected, None, &render_options)
            .map_err(Failure::runtime)?
    } else {
        Vec::new()
    }
    .into_iter();

    let dest_links = if r.outputs & PDF_OUT_ITEMS != 0 {
        super::links::dest_items(&storage, doc, &selected, state, frame)
    } else {
        Vec::new()
    };

    let mut page_views = Vec::new();
    let mut markdown_parts = Vec::new();
    let mut text_parts = Vec::new();
    for number in &selected {
        let page_frame = state.frame(*number)?;
        let frame_height = page_frame.height(frame);
        #[cfg(feature = "render-pdfium")]
        let (render_x0, render_y1) = (page_frame.sheet.x0, page_frame.sheet.y1);
        let mut p = PdfPage {
            info: page_frame.info(frame),
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
        if view.present & PDF_OUT_ANALYSIS != 0
            && (garbled(reasons)
                || native_page.is_some_and(|p| {
                    p.ocr_reason.as_deref()
                        == Some(pdf_inspector::OCR_REASON_SUSPECTED_GARBLED_TEXT)
                }))
        {
            view.flags |= PDF_DOC_ENCODING_ISSUES;
            p.flags |= PDF_PAGE_ENCODING_ISSUES;
        }
        let final_page = final_pages.get(number);
        if let Some(f) = final_page {
            p.provenance = provenance(&storage, &f.provenance)?;
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
        if r.outputs & PDF_OUT_MARKDOWN != 0 {
            p.markdown = storage.bytes(page_md);
            markdown_parts.push((*number, page_md.to_owned()));
        }
        if let Some(content) = &content {
            let page_items = content
                .items
                .iter()
                .filter(|i| i.page == *number)
                .cloned()
                .collect::<Vec<pdf_inspector::TextItem>>();
            if r.outputs & PDF_OUT_ITEMS != 0 {
                p.items = storage.slice(
                    page_items
                        .iter()
                        .map(|i| storage.item(i, frame_height))
                        .chain(
                            dest_links
                                .iter()
                                .filter(|item| item.page == *number)
                                .copied(),
                        ),
                );
            }
            if let Some(text) = sheet_text.remove(number) {
                p.text = storage.bytes(&text);
                text_parts.push(text);
            }
            if r.outputs & PDF_OUT_GEOMETRY != 0 {
                p.rectangles =
                    storage.slice(content.rects.iter().filter(|v| v.page == *number).map(|v| {
                        PdfRectangle {
                            page: *number,
                            bounds: flip_box(v.x, v.y, v.width, v.height, frame_height),
                        }
                    }));
                p.lines =
                    storage.slice(content.lines.iter().filter(|v| v.page == *number).map(|v| {
                        PdfSegment {
                            page: *number,
                            start: flip_point(v.x1, v.y1, frame_height),
                            end: flip_point(v.x2, v.y2, frame_height),
                        }
                    }));
            }
            if content.gid_pages.contains(number) {
                p.flags |= PDF_PAGE_GID_ENCODED;
            }
            let has_table = p.flags & PDF_PAGE_HAS_TABLES != 0;
            let (columns, newspaper) = super::layout::page_columns(&page_items, *number, has_table);
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
            p.columns = storage.slice(columns.into_iter().map(|(x0, x1)| PdfInterval {
                x0: x0.min(x1),
                x1: x0.max(x1),
            }));
            p.charts = storage.slice(
                pdf_inspector::tables::detect_chart_regions(&page_items, &content.rects, *number)
                    .into_iter()
                    .map(|(x, y, w, h)| flip_box(x, y, w, h, frame_height)),
            );
            // The core records only turned pages; absence means upright.
            let turned = content.rotations.get(number).copied();
            p.text_orientation = orientation(turned.unwrap_or(PageRotation::Upright));
        }
        if r.outputs & PDF_OUT_STRUCTURE != 0 {
            p.structure = storage.slice(elements.iter().filter(|e| e.page == *number).copied());
        }
        #[cfg(feature = "render-pdfium")]
        if let Some(page) = rendered.next() {
            p.image = image_view(&storage, page, render_x0, render_y1)?;
        }
        page_views.push(p);
    }
    if r.outputs & PDF_OUT_MARKDOWN != 0 {
        let document_markdown = match ocr.markdown {
            Some(markdown) if external.is_empty() => markdown,
            _ => assemble(&markdown_parts, md.include_page_numbers),
        };
        view.markdown = storage.owned(document_markdown.into_bytes());
    }
    if r.outputs & PDF_OUT_TEXT != 0 {
        view.text = storage.owned(text_parts.join("\n\n").into_bytes());
    }
    let region_views = region_queries(&storage, state, regions, position)?;
    view.regions = storage.slice(region_views);
    if !tables.is_empty() {
        view.tables = storage.slice(super::tables::hinted_tables(
            &storage,
            &state.bytes,
            tables,
            table_inputs,
        )?);
    }
    view.pages = storage.slice(page_views);
    if let (Some(tree), true) = (state.structure(), r.outputs & PDF_OUT_STRUCTURE != 0) {
        view.structure_nodes = super::semantic::nodes(&storage, tree, &state.pages, &selected);
    }
    view.processing_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    Ok(ResultOwner::new(view, storage))
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
/// Plain text of one page's sheet-frame runs, one grouped line per row,
/// with formatting cleared so `TextLine::text` emits no markup.
fn plain_text(items: &[pdf_inspector::TextItem], page: u32) -> String {
    pdf_inspector::extractor::group_into_lines_preserving_all_text(
        items.iter().filter(|i| i.page == page).cloned().collect(),
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
    .join("\n")
}
fn garbled(reasons: &[String]) -> bool {
    reasons
        .iter()
        .any(|reason| reason == pdf_inspector::OCR_REASON_SUSPECTED_GARBLED_TEXT)
}
/// True when `d` is what `pdf_inspector_request_init` sets, byte for byte in
/// the scalar fields and with no explicit detector page list.
fn is_default_detection(d: PdfDetectionOptions) -> bool {
    let default = default_request().detection;
    d.strategy == default.strategy
        && d.sample_size == default.sample_size
        && d.min_text_ops == default.min_text_ops
        && d.text_page_ratio.to_bits() == default.text_page_ratio.to_bits()
        && d.pages.len == 0
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
fn provenance(s: &Storage, p: &pdf_inspector::vision::PageProvenance) -> Fallible<PdfProvenance> {
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

/// Run text and table region queries as one batched core call per kind and
/// vector-grid queries individually, preserving descriptor order.
///
/// `position` is the caller-visible frame of `regions` plus bold-from-weight
/// knobs: display rects are read in that frame, while views echo the
/// caller's original bounds.
fn region_queries(
    s: &Storage,
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
            views[i].text = s.owned(region.text.into_bytes());
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
            views[i].cells = s.slice(grid.cell_bboxes.into_iter().map(|b| PdfBox {
                x0: b[0] + bbox[0],
                y0: b[1] + bbox[1],
                x1: b[2] + bbox[0],
                y1: b[3] + bbox[1],
            }));
        }
    }
    Ok(views)
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
        page(e.page, state.count())?;
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
            super::tables::validate_quad(s.polygon)?;
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
            rendered: phantom_page(state.frame(e.page)?.info(PositionFrame::Sheet))?,
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
fn image_view(s: &Storage, r: RenderedPage, sheet_x0: f32, sheet_y1: f32) -> Fallible<PdfImage> {
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
        pixels: s.owned(r.into_pixels()),
        pixel_to_page: transform,
        page_to_pixel: transform.inverse()?,
    })
}

const ALL_ITEM_FLAGS: u32 = PDF_BOLD
    | PDF_ITALIC
    | PDF_UNDERLINE
    | PDF_STRIKEOUT
    | PDF_HAS_MCID
    | PDF_ADVANCE_KNOWN
    | PDF_LEGACY_SYMBOL_REWRITE
    | PDF_HAS_FILL_COLOR
    | PDF_HAS_STROKE_COLOR
    | PDF_HAS_RENDER_MODE;
fn markdown_result(markdown: String, page_count: u32) -> ResultOwner {
    let storage = Storage::default();
    let result = PdfResult {
        present: PDF_OUT_MARKDOWN,
        page_count,
        markdown: storage.owned(markdown.into_bytes()),
        ..PdfResult::default()
    };
    ResultOwner::new(result, storage)
}
pub(super) unsafe fn compose_text(
    text: PdfBytes,
    options: pdf_inspector::MarkdownOptions,
) -> Fallible<ResultOwner> {
    Ok(markdown_result(
        pdf_inspector::to_markdown(input::text(text)?, options),
        0,
    ))
}
pub(super) unsafe fn compose_items(
    c: &PdfComposeInput,
    options: pdf_inspector::MarkdownOptions,
) -> Fallible<ResultOwner> {
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
        reserved(i.flags, ALL_ITEM_FLAGS, "item flags")?;
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
            fill_color: if i.flags & PDF_HAS_FILL_COLOR != 0 {
                Some(super::output::parse_color(i.fill_color, "fill_color")?)
            } else {
                None
            },
            stroke_color: if i.flags & PDF_HAS_STROKE_COLOR != 0 {
                Some(super::output::parse_color(i.stroke_color, "stroke_color")?)
            } else {
                None
            },
            render_mode: if i.flags & PDF_HAS_RENDER_MODE != 0 {
                Some(super::output::parse_render_mode(i.render_mode)?)
            } else {
                None
            },
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
    Ok(markdown_result(value, count))
}
