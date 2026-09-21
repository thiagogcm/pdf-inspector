use super::*;
use std::ptr::{null, null_mut};
use std::sync::{Arc, Barrier};
mod workflows;

fn view(bytes: &[u8]) -> PdfBytes {
    PdfBytes {
        ptr: bytes.as_ptr(),
        len: bytes.len(),
    }
}
unsafe fn string(v: PdfBytes) -> String {
    String::from_utf8(input::bytes(v).unwrap().to_vec()).unwrap()
}
struct Doc(*mut PdfDocument);
impl Doc {
    fn open(name: &str) -> Self {
        let bytes = std::fs::read(format!("../tests/fixtures/{name}.pdf")).unwrap();
        Self::bytes(&bytes, None)
    }
    fn bytes(bytes: &[u8], password: Option<&str>) -> Self {
        unsafe {
            let mut doc = null_mut();
            let mut err = null_mut();
            let source = PdfSource {
                kind: PDF_SOURCE_BYTES,
                data: view(bytes),
                password: password.map_or(PdfBytes::default(), |p| view(p.as_bytes())),
            };
            let status = pdf_inspector_open(&source, &mut doc, &mut err);
            assert_success(status, err);
            Self(doc)
        }
    }
    fn run(&self, r: &PdfRequest) -> Owned {
        unsafe {
            let mut out = null_mut();
            let mut e = null_mut();
            let status = pdf_inspector_execute(self.0, r, &mut out, &mut e);
            assert_success(status, e);
            Owned(out)
        }
    }
    fn info(&self) -> &PdfDocumentInfo {
        unsafe { &*pdf_inspector_document_info(self.0) }
    }
}
impl Drop for Doc {
    fn drop(&mut self) {
        unsafe { pdf_inspector_document_free(self.0) }
    }
}
struct Owned(*mut PdfResult);
impl Owned {
    fn get(&self) -> &PdfResult {
        unsafe { &*self.0 }
    }
    fn pages(&self) -> &[PdfPage] {
        unsafe { input::slice(self.get().pages.ptr, self.get().pages.len).unwrap() }
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { pdf_inspector_result_free(self.0) }
    }
}
unsafe fn assert_success(status: i32, error: *mut PdfError) {
    let message = if error.is_null() {
        String::new()
    } else {
        string((*error).message)
    };
    pdf_inspector_error_free(error);
    assert_eq!(status, PDF_OK, "{message}");
}
unsafe fn expect_failure(doc: *const PdfDocument, r: &PdfRequest, expected: i32) -> *mut PdfError {
    let mut result = std::ptr::dangling_mut();
    let mut error = null_mut();
    assert_eq!(
        pdf_inspector_execute(doc, r, &mut result, &mut error),
        expected
    );
    assert!(result.is_null());
    assert!(!error.is_null());
    assert_eq!((*error).status, expected);
    error
}
fn open(mut doc: lopdf::Document) -> Doc {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    Doc::bytes(&bytes, None)
}
fn compose_ok(c: &PdfComposeInput, options: *const PdfMarkdownOptions) -> Owned {
    unsafe {
        let mut out = null_mut();
        let mut e = null_mut();
        assert_success(pdf_inspector_compose_items(c, options, &mut out, &mut e), e);
        Owned(out)
    }
}
fn compose_err(c: &PdfComposeInput, options: *const PdfMarkdownOptions) -> i32 {
    unsafe {
        let mut out = std::ptr::dangling_mut();
        let mut e = null_mut();
        let status = pdf_inspector_compose_items(c, options, &mut out, &mut e);
        assert!(out.is_null() && !e.is_null());
        pdf_inspector_error_free(e);
        status
    }
}

#[test]
fn owned_source_reusable_document_and_independent_snapshots() {
    let doc = Doc::open("bare_name_struct");
    let mut r = input::default_request();
    r.outputs |= PDF_OUT_ITEMS | PDF_OUT_STRUCTURE | PDF_OUT_TEXT | PDF_OUT_GEOMETRY;
    let first = doc.run(&r);
    let initial = unsafe { string(first.get().markdown) };
    assert!(initial.contains("# Test"));
    assert!(!first.pages()[0].items.ptr.is_null());
    r.markdown.flags &= !PDF_MD_HEADERS;
    let second = doc.run(&r);
    assert!(!unsafe { string(second.get().markdown) }.contains("# Test"));
    assert!(!unsafe { string(second.pages()[0].markdown) }.contains("# Test"));
    drop(doc);
    assert_eq!(unsafe { string(first.get().markdown) }, initial);
    assert!(unsafe { string(second.get().text) }.contains("Test"));
    let page = &first.pages()[0];
    let items = unsafe { input::slice(page.items.ptr, page.items.len).unwrap() };
    let roles = unsafe { input::slice(page.structure.ptr, page.structure.len).unwrap() };
    assert!(items.iter().any(|i| i.flags & PDF_HAS_MCID != 0
        && roles.iter().any(|r| r.mcid == i.mcid && r.page == i.page)));
}

#[test]
fn source_path_is_read_only_at_open() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("source.pdf");
    std::fs::copy("../tests/fixtures/bare_name_struct.pdf", &p).unwrap();
    let name = p.to_str().unwrap();
    let source = PdfSource {
        kind: PDF_SOURCE_PATH,
        data: view(name.as_bytes()),
        password: PdfBytes::default(),
    };
    let mut doc = null_mut();
    let mut err = null_mut();
    unsafe {
        assert_success(pdf_inspector_open(&source, &mut doc, &mut err), err);
    }
    let doc = Doc(doc);
    std::fs::remove_file(p).unwrap();
    assert!(!doc
        .run(&input::default_request())
        .get()
        .markdown
        .ptr
        .is_null());
}

#[test]
fn inspection_does_not_extract_text() {
    let doc = Doc::open("bare_name_struct");
    let mut r = input::default_request();
    r.outputs = PDF_OUT_INSPECTION;
    let result = doc.run(&r);
    assert!(result.get().markdown.ptr.is_null());
    assert_eq!(result.get().present & PDF_OUT_ANALYSIS, 0);
    assert!(result.pages()[0].items.ptr.is_null());
    assert!(result.get().tables.ptr.is_null());
    assert!(result.pages()[0].markdown.ptr.is_null());
}

#[test]
fn errors_belong_to_calls_across_threads_and_successes() {
    let doc = Doc::open("bare_name_struct");
    let mut a = input::default_request();
    a.outputs = u32::MAX;

    unsafe {
        let first = expect_failure(doc.0, &a, PDF_INVALID_ARGUMENT);
        let message = string((*first).message);
        let address = doc.0 as usize;
        let barrier = Arc::new(Barrier::new(2));
        let other = barrier.clone();
        let thread = std::thread::spawn(move || {
            let mut b = input::default_request();
            b.ocr.minimum_confidence = f32::NAN;
            let e = expect_failure(address as *const PdfDocument, &b, PDF_INVALID_ARGUMENT);
            other.wait();
            let message = string((*e).message);
            pdf_inspector_error_free(e);
            message
        });
        barrier.wait();
        let second = thread.join().unwrap();
        assert_ne!(message, second);
        let _result = doc.run(&input::default_request());
        assert_eq!(message, string((*first).message));
        pdf_inspector_error_free(first);
    }
}

#[test]
fn document_calls_can_move_between_threads() {
    let doc = Doc::open("bare_name_struct");
    let address = doc.0 as usize;
    let threads = (0..4)
        .map(|_| {
            std::thread::spawn(move || unsafe {
                let mut out = null_mut();
                let mut err = null_mut();
                let mut r = input::default_request();
                r.outputs |= PDF_OUT_ITEMS | PDF_OUT_STRUCTURE | PDF_OUT_TABLES;
                assert_success(
                    pdf_inspector_execute(address as *const PdfDocument, &r, &mut out, &mut err),
                    err,
                );
                let text = string((*out).markdown);
                pdf_inspector_result_free(out);
                text
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        assert!(thread.join().unwrap().contains("# Test"));
    }
}

#[test]
fn rejects_invalid_descriptors_without_partial_output() {
    let doc = Doc::open("bare_name_struct");
    let zero = [0];
    let too_large = [u32::MAX];
    let mut requests = Vec::new();
    let mut r = input::default_request();
    r.pages = PdfPageNumbers {
        ptr: zero.as_ptr(),
        len: 1,
    };
    requests.push(r);
    r.pages = PdfPageNumbers {
        ptr: too_large.as_ptr(),
        len: 1,
    };
    requests.push(r);
    r = input::default_request();
    r.pages = PdfPageNumbers {
        ptr: std::ptr::dangling(),
        len: usize::MAX,
    };
    requests.push(r);
    r = input::default_request();
    r.regions = PdfRegionInputs {
        ptr: null(),
        len: 1,
    };
    requests.push(r);
    r = input::default_request();
    r.markdown.flags = u32::MAX;
    requests.push(r);
    r = input::default_request();
    r.ocr.download_policy = u32::MAX;
    requests.push(r);
    r = input::default_request();
    r.outputs = 512;
    requests.push(r);
    r = input::default_request();
    r.flags = 4;
    requests.push(r);
    r = input::default_request();
    r.render.flags = 4;
    requests.push(r);
    r = input::default_request();
    r.render.dpi = f32::INFINITY;
    requests.push(r);
    r = input::default_request();
    r.ocr.model_directory = PdfBytes {
        ptr: null(),
        len: 1,
    };
    requests.push(r);
    for r in requests {
        unsafe {
            let e = expect_failure(doc.0, &r, PDF_INVALID_ARGUMENT);
            pdf_inspector_error_free(e);
        }
    }
}

#[test]
fn password_and_null_source_failures_are_owned() {
    let bytes = std::fs::read("../tests/fixtures/encrypted-secret123.pdf").unwrap();
    let source = PdfSource {
        kind: PDF_SOURCE_BYTES,
        data: view(&bytes),
        password: PdfBytes::default(),
    };
    unsafe {
        let mut out = std::ptr::dangling_mut();
        let mut e = null_mut();
        assert_eq!(
            pdf_inspector_open(&source, &mut out, &mut e),
            PDF_PASSWORD_ERROR
        );
        assert!(out.is_null());
        pdf_inspector_error_free(e);
        assert_eq!(
            pdf_inspector_open(null(), &mut out, &mut e),
            PDF_INVALID_ARGUMENT
        );
        assert!(out.is_null());
        pdf_inspector_error_free(e);
    }
    let doc = Doc::bytes(&bytes, Some("secret123"));
    let mut r = input::default_request();
    r.outputs |= PDF_OUT_ITEMS | PDF_OUT_STRUCTURE | PDF_OUT_GEOMETRY | PDF_OUT_TEXT;
    let region = PdfRegionInput {
        page: 1,
        kind: PDF_REGION_TEXT,
        bounds: PdfBox {
            x0: 0.0,
            y0: 0.0,
            x1: 600.0,
            y1: 800.0,
        },
    };
    r.regions = PdfRegionInputs {
        ptr: &region,
        len: 1,
    };
    let result = doc.run(&r);
    let markdown = unsafe { string(result.get().markdown) };
    assert!(!markdown.trim().is_empty(), "{markdown:?}");
    let page = &result.pages()[0];
    assert!(page.items.len > 0 && !page.text.ptr.is_null());
    let regions = unsafe { input::slice(result.get().regions.ptr, 1).unwrap() };
    let region_text = unsafe { string(regions[0].text) };
    assert!(region_text.contains("Procurement"), "{region_text:?}");
    assert!(unsafe { string(page.text) }.contains("Procurement"));
    assert_eq!(result.pages()[0].flags & PDF_PAGE_NEEDS_OCR, 0);
}

#[test]
fn plain_composition_preserves_nul_and_rejects_invalid_utf8() {
    unsafe {
        let text = b"plain\0text";
        let mut options = PdfMarkdownOptions::default();
        assert_eq!(pdf_inspector_markdown_options_init(&mut options), PDF_OK);
        assert_eq!(options.flags, input::default_markdown().flags);
        let mut out = null_mut();
        let mut e = null_mut();
        assert_success(
            pdf_inspector_compose_text(view(text), &options, &mut out, &mut e),
            e,
        );
        let result = Owned(out);
        assert!(string(result.get().markdown).contains("plain\0text"));
        assert_eq!(
            pdf_inspector_compose_text(view(&[255]), null(), &mut out, &mut e),
            PDF_INVALID_ARGUMENT
        );
        assert!(out.is_null());
        pdf_inspector_error_free(e);
        assert_success(
            pdf_inspector_compose_text(view(b""), null(), &mut out, &mut e),
            e,
        );
        let empty = Owned(out);
        assert!(!empty.get().markdown.ptr.is_null());
        assert_eq!(empty.get().markdown.len, 0);
    }
}

#[test]
fn positioned_geometry_and_scripts_survive_bulk_views_and_composition() {
    for name in [
        "cropbox_offset_origin",
        "rotated_margin_stamp",
        "author_block_superscripts",
    ] {
        let doc = Doc::open(name);
        let mut r = input::default_request();
        r.outputs |= PDF_OUT_ITEMS | PDF_OUT_STRUCTURE | PDF_OUT_GEOMETRY;
        let result = doc.run(&r);
        let pages = result.pages();
        let mut all = Vec::new();
        let mut infos = Vec::new();
        for p in pages {
            infos.push(p.info);
            let items = unsafe { input::slice(p.items.ptr, p.items.len).unwrap() };
            all.extend_from_slice(items);
            assert!(items
                .iter()
                .all(|i| i.bounds.x1 >= i.bounds.x0 && i.bounds.y1 >= i.bounds.y0));
        }
        if name == "author_block_superscripts" {
            assert!(all.iter().any(|i| i.baseline_shift > 0.0));
        }
        if name == "rotated_margin_stamp" {
            assert!(all
                .iter()
                .any(|i| i.rotation == 270.0
                    && i.bounds.y1 - i.bounds.y0 > i.bounds.x1 - i.bounds.x0));
        }
        let first = all
            .iter()
            .find(|i| unsafe { !string(i.text).trim().is_empty() } && i.bounds.x1 > i.bounds.x0)
            .unwrap();
        let region = PdfRegionInput {
            page: first.page,
            kind: PDF_REGION_TEXT,
            bounds: PdfBox {
                x0: first.bounds.x0 - 1.0,
                y0: first.bounds.y0 - 1.0,
                x1: first.bounds.x1 + 1.0,
                y1: first.bounds.y1 + 1.0,
            },
        };
        r.regions = PdfRegionInputs {
            ptr: &region,
            len: 1,
        };
        let query = doc.run(&r);
        let regions = unsafe { input::slice(query.get().regions.ptr, 1).unwrap() };
        let expected = unsafe { string(first.text) };
        let actual = unsafe { string(regions[0].text) };
        assert!(
            actual.contains(expected.trim()),
            "{name}: expected {expected:?}, got {actual:?}"
        );
        let mut c = PdfComposeInput {
            pages: PdfPageInfos {
                ptr: infos.as_ptr(),
                len: infos.len(),
            },
            items: PdfItems {
                ptr: all.as_ptr(),
                len: all.len(),
            },
            ..PdfComposeInput::default()
        };
        let composed = compose_ok(&c, null());
        assert!(unsafe { string(composed.get().markdown) }.contains(expected.trim()));
        c.items = PdfItems {
            ptr: null(),
            len: 1,
        };
        assert_eq!(compose_err(&c, null()), PDF_INVALID_ARGUMENT);
    }
}

fn symbol_rewrite_pdf() -> lopdf::Document {
    use lopdf::{dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let cmap = doc.add_object(Stream::new(
        dictionary! {},
        br#"
/CIDInit /ProcSet findresource begin 12 dict begin begincmap
/CMapName /ExampleMap def /CMapType 2 def
1 begincodespacerange <00> <FF> endcodespacerange
3 beginbfchar
<41> <0041> <57> <F057> <42> <0042>
endbfchar endcmap CMapName currentdict /CMap defineresource pop end end
"#
        .to_vec(),
    ));
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "ExampleFace",
        "FirstChar" => 0, "LastChar" => 255,
        "Widths" => Object::Array((0..=255).map(|_| 600.into()).collect()),
        "ToUnicode" => cmap,
    });
    let stream = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 10 Tf 50 300 Td [(A) -3000 (W) -3000.0 (B)] TJ ET".to_vec(),
    ));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), 600.into(), 400.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        "Contents" => stream,
    });
    doc.objects.insert(
        pages,
        dictionary! { "Type" => "Pages", "Count" => 1, "Kids" => vec![page.into()] }.into(),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);
    doc
}

#[test]
fn legacy_symbol_rewrite_is_item_flag_and_survives_composition() {
    let doc = open(symbol_rewrite_pdf());
    let mut r = input::default_request();
    r.outputs = PDF_OUT_ITEMS;
    let result = doc.run(&r);
    let items = unsafe {
        let page = &result.pages()[0];
        input::slice(page.items.ptr, page.items.len).unwrap()
    };
    let flags: Vec<_> = items
        .iter()
        .map(|i| {
            (
                unsafe { string(i.text) },
                i.flags & PDF_LEGACY_SYMBOL_REWRITE != 0,
            )
        })
        .collect();
    assert_eq!(
        flags,
        vec![("A".into(), false), ("W".into(), true), ("B".into(), false)]
    );
    let info = result.pages()[0].info;
    let input = PdfComposeInput {
        pages: PdfPageInfos { ptr: &info, len: 1 },
        items: PdfItems {
            ptr: items.as_ptr(),
            len: items.len(),
        },
        ..PdfComposeInput::default()
    };
    compose_ok(&input, null());
}

#[test]
fn cropped_page_coordinates_ignore_inherited_display_rotation() {
    for vertical in [false, true] {
        for rotation in [0, 90, 180, 270] {
            let mut source =
                lopdf::Document::load("../tests/fixtures/cropbox_offset_origin.pdf").unwrap();
            source
                .get_object_mut((2, 0))
                .unwrap()
                .as_dict_mut()
                .unwrap()
                .set("Rotate", rotation);
            if vertical {
                source
                    .get_object_mut((4, 0))
                    .unwrap()
                    .as_stream_mut()
                    .unwrap()
                    .set_content(
                        b"BT /F1 12 Tf 0 1 -1 0 120 300 Tm (Visible glyph) Tj ET".to_vec(),
                    );
            }
            let doc = open(source);
            let mut r = input::default_request();
            r.outputs = PDF_OUT_ITEMS | PDF_OUT_TEXT;
            #[cfg(feature = "render-pdfium")]
            if std::env::var_os("PDFIUM_LIB_PATH").is_some() {
                r.outputs |= PDF_OUT_RENDER;
            }
            let result = doc.run(&r);
            let p = &result.pages()[0];
            assert_eq!(
                (p.info.width, p.info.height, p.info.rotation),
                (300.0, 400.0, rotation as u32)
            );
            let items = unsafe { input::slice(p.items.ptr, p.items.len).unwrap() };
            let item = items
                .iter()
                .find(|i| unsafe { string(i.text) }.contains("Visible"))
                .unwrap();
            assert!(
                (item.bounds.x0 - if vertical { 58.0 } else { 70.0 }).abs() < 0.01,
                "{item:?}"
            );
            assert_eq!(item.rotation, if vertical { 270.0 } else { 0.0 });
            // A lone rotated run is a stamp, not a rotated page.
            assert_eq!(p.text_orientation, PDF_ORIENTATION_UPRIGHT);
            assert!(item.flags & PDF_ADVANCE_KNOWN != 0);
            if !vertical {
                assert!((item.bounds.y0 - 148.0).abs() < 0.01);
            }
            let region = PdfRegionInput {
                page: 1,
                kind: PDF_REGION_TEXT,
                bounds: item.bounds,
            };
            r.regions = PdfRegionInputs {
                ptr: &region,
                len: 1,
            };
            let queried = doc.run(&r);
            let regions = unsafe { input::slice(queried.get().regions.ptr, 1).unwrap() };
            assert!(unsafe { string(regions[0].text) }.contains("Visible glyph"));
            if r.outputs & PDF_OUT_RENDER != 0 {
                let im = p.image;
                let corners = [
                    (0.0, 0.0),
                    (im.width as f64, 0.0),
                    (0.0, im.height as f64),
                    (im.width as f64, im.height as f64),
                ];
                for (x, y) in corners {
                    let (px, py) = im.pixel_to_page.apply(x, y);
                    assert!(px.abs().min((px - 300.0).abs()) < 0.01, "{rotation}: {px}");
                    assert!(py.abs().min((py - 400.0).abs()) < 0.01, "{rotation}: {py}");
                    let (rx, ry) = im.page_to_pixel.apply(px, py);
                    assert!((rx - x).abs() < 0.01 && (ry - y).abs() < 0.01);
                }
            }
        }
    }
}

fn with_rotate(rotation: i32) -> Doc {
    let mut source = lopdf::Document::load("../tests/fixtures/cropbox_offset_origin.pdf").unwrap();
    source
        .get_object_mut((2, 0))
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Rotate", rotation);
    open(source)
}

#[test]
fn predominantly_rotated_text_reports_orientation() {
    let mut source = lopdf::Document::load("../tests/fixtures/cropbox_offset_origin.pdf").unwrap();
    source
        .get_object_mut((4, 0))
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_content(
            b"BT /F1 12 Tf 0 1 -1 0 120 300 Tm (First) Tj 0 1 -1 0 140 300 Tm (Second) Tj ET"
                .to_vec(),
        );
    let doc = open(source);
    let mut r = input::default_request();
    r.outputs = PDF_OUT_INSPECTION;
    assert_eq!(
        doc.run(&r).pages()[0].text_orientation,
        PDF_ORIENTATION_UNKNOWN
    );
    for outputs in [PDF_OUT_ITEMS, PDF_OUT_TABLES] {
        r.outputs = outputs;
        assert_eq!(doc.run(&r).pages()[0].text_orientation, PDF_ORIENTATION_CCW);
    }
}

#[test]
fn null_output_slots_are_rejected() {
    let doc = Doc::open("bare_name_struct");
    unsafe {
        assert_eq!(pdf_inspector_request_init(null_mut()), PDF_INVALID_ARGUMENT);
        assert_eq!(
            pdf_inspector_runtime_options_init(null_mut()),
            PDF_INVALID_ARGUMENT
        );
        assert_eq!(
            pdf_inspector_markdown_options_init(null_mut()),
            PDF_INVALID_ARGUMENT
        );
        let mut error = null_mut();
        assert_eq!(
            pdf_inspector_execute(doc.0, null(), null_mut(), &mut error),
            PDF_INVALID_ARGUMENT
        );
        assert!(!error.is_null());
        pdf_inspector_error_free(error);
        assert_eq!(
            pdf_inspector_execute(doc.0, null(), null_mut(), null_mut()),
            PDF_INVALID_ARGUMENT
        );
    }
}

#[test]
fn request_init_defaults_to_sheet_frame() {
    let mut request = PdfRequest {
        outputs: 0xdead_beef,
        frame: 0xdead_beef,
        flags: 0xdead_beef,
        bold_weight_threshold: 0,
        ..PdfRequest::default()
    };
    unsafe {
        assert_success(pdf_inspector_request_init(&mut request), null_mut());
    }
    assert_eq!(request.frame, PDF_FRAME_SHEET);
    assert_eq!(request.flags, 0);
    assert_eq!(request.bold_weight_threshold, 600);
    assert_eq!(
        request.render.flags,
        PDF_RENDER_ANNOTATIONS | PDF_RENDER_FORM_FIELDS
    );
}

#[test]
fn unknown_frame_is_rejected() {
    let doc = with_rotate(0);
    let mut r = input::default_request();
    r.outputs = PDF_OUT_ITEMS;
    r.frame = 99;
    unsafe {
        let error = expect_failure(doc.0, &r, PDF_INVALID_ARGUMENT);
        pdf_inspector_error_free(error);
    }
}

#[test]
fn known_request_flags_are_accepted() {
    let doc = with_rotate(0);
    let mut r = input::default_request();
    r.outputs = PDF_OUT_ITEMS;
    r.flags = PDF_REQUEST_BOLD_FROM_WEIGHT | PDF_REQUEST_INCLUDE_INVISIBLE;
    assert!(doc.run(&r).pages()[0].items.len > 0);
}

#[test]
fn bold_weight_threshold_outside_scale_is_rejected() {
    let doc = with_rotate(0);
    for threshold in [99, 901] {
        let mut r = input::default_request();
        r.outputs = PDF_OUT_ITEMS;
        r.bold_weight_threshold = threshold;
        unsafe {
            let error = expect_failure(doc.0, &r, PDF_INVALID_ARGUMENT);
            pdf_inspector_error_free(error);
        }
    }
}

fn sample_text_item() -> pdf_inspector::TextItem {
    pdf_inspector::TextItem {
        text: "Heavy".into(),
        x: 10.0,
        y: 20.0,
        width: 30.0,
        height: 12.0,
        rotation: 0.0,
        advance_known: true,
        font: "Test".into(),
        font_tag: "F1".into(),
        legacy_symbol_rewrite: false,
        font_size: 12.0,
        page: 1,
        is_bold: true,
        is_italic: false,
        font_weight: Some(700),
        bold_source: Some(pdf_inspector::BoldSource::FontName),
        fixed_pitch: Some(true),
        is_underline: false,
        is_strikeout: false,
        item_type: pdf_inspector::types::ItemType::Text,
        mcid: None,
        baseline_shift: 0.0,
    }
}

#[test]
fn item_view_copies_font_metadata() {
    let storage = super::output::Storage::default();
    let item = storage.item(&sample_text_item(), 100.0);
    assert_eq!(item.font_weight, 700);
    assert_eq!(item.bold_source, PDF_BOLD_FONT_NAME);
    assert_eq!(item.fixed_pitch, PDF_PITCH_FIXED);
    let blank = pdf_inspector::TextItem {
        is_bold: false,
        font_weight: None,
        bold_source: None,
        fixed_pitch: None,
        ..sample_text_item()
    };
    let item = storage.item(&blank, 100.0);
    assert_eq!(item.font_weight, 0);
    assert_eq!(item.bold_source, 0);
    assert_eq!(item.fixed_pitch, PDF_PITCH_UNKNOWN);
    assert_eq!(item.dest_page, 0);
}

#[test]
fn inspection_forwards_routing_sample_stats_and_load_audit() {
    let doc = Doc::open("bare_name_struct");
    let mut r = input::default_request();
    r.outputs = PDF_OUT_INSPECTION;
    let result = doc.run(&r);
    let view = result.get();
    assert!(view.pages_sampled > 0);
    assert!(view.pages_with_text <= view.pages_sampled);
    assert_eq!(
        view.flags & !(PDF_DOC_ENCODING_ISSUES | PDF_DOC_OCR_RECOMMENDED),
        0
    );
    let info = doc.info();
    assert_eq!(info.page_count, 1);
    assert_eq!(info.audit.leading_bytes, 0);
    assert_eq!(info.audit.flags & PDF_LOAD_LEADING_BYTES, 0);
    assert_eq!(info.audit.flags & PDF_LOAD_DECRYPTED, 0);
    let page = &result.pages()[0];
    assert_eq!(page.quality.alphanumeric_chars, 0);
    assert_eq!(page.columns.len, 0);
}

#[test]
fn load_audit_counts_leading_bytes_before_header() {
    let mut bytes = b"% comment\n".to_vec();
    bytes.extend_from_slice(&std::fs::read("../tests/fixtures/bare_name_struct.pdf").unwrap());
    let doc = Doc::bytes(&bytes, None);
    assert_eq!(doc.info().audit.leading_bytes, 10);
    assert_ne!(doc.info().audit.flags & PDF_LOAD_LEADING_BYTES, 0);
}

#[test]
fn document_info_exposes_sheet_pages_and_audit() {
    assert!(unsafe { pdf_inspector_document_info(null()) }.is_null());
    let doc = with_rotate(90);
    let info = doc.info();
    let pages = unsafe { input::slice(info.pages.ptr, info.pages.len).unwrap() };
    assert_eq!(info.page_count as usize, pages.len());
    assert_eq!(
        (
            pages[0].page,
            pages[0].width,
            pages[0].height,
            pages[0].rotation
        ),
        (1, 300.0, 400.0, 90)
    );
    assert!(!unsafe { string(pdf_inspector_version()) }.is_empty());
}

#[test]
fn analysis_forwards_native_quality_numbers() {
    let doc = Doc::open("bare_name_struct");
    let mut r = input::default_request();
    r.outputs = PDF_OUT_ANALYSIS | PDF_OUT_ITEMS;
    let result = doc.run(&r);
    let page = &result.pages()[0];
    assert!(page.quality.alphanumeric_chars > 0);
    assert!(page.quality.visible_chars >= page.quality.alphanumeric_chars);
    assert!(page.quality.density > 0.0);
    assert!(page.quality.score > 0.0);
    assert_eq!(page.reading_order, PDF_READING_SINGLE);
    assert_eq!(page.flags & PDF_PAGE_HAS_COLUMNS, 0);
    let columns = unsafe { input::slice(page.columns.ptr, page.columns.len).unwrap() };
    assert!(columns.len() <= 1);
    assert!(columns.iter().all(|column| column.x1 >= column.x0));
    let items = unsafe { input::slice(page.items.ptr, page.items.len).unwrap() };
    assert!(items.iter().all(|item| item.dest_page == 0));
}

#[test]
fn quality_view_forwards_metrics() {
    let metrics = pdf_inspector::text_quality_metrics("Hello world");
    let view = super::output::quality_view(metrics);
    assert_eq!(view.alphanumeric_chars, metrics.alphanumeric_chars);
    assert_eq!(view.visible_chars, metrics.visible_chars);
    assert_eq!(view.density, metrics.density);
    assert_eq!(view.replacement_chars, metrics.replacement_chars);
    assert_eq!(
        view.longest_replacement_run,
        metrics.longest_replacement_run
    );
    assert_eq!(view.english_cosine, metrics.english_cosine);
    assert_eq!(view.score, metrics.score);
    assert!(view.alphanumeric_chars > 0);
    assert!(view.score > 0.0);
}

#[test]
fn intervals_and_floats_are_stored() {
    let storage = super::output::Storage::default();
    let columns = storage.slice::<PdfIntervals>(vec![PdfInterval { x0: 10.0, x1: 40.0 }]);
    let columns = unsafe { input::slice(columns.ptr, columns.len).unwrap() };
    assert_eq!(columns.len(), 1);
    assert_eq!(columns[0].x0, 10.0);
    assert_eq!(columns[0].x1, 40.0);
    let edges = storage.slice::<PdfFloats>(vec![1.0, 2.5, 4.0]);
    let edges = unsafe { input::slice(edges.ptr, edges.len).unwrap() };
    assert_eq!(edges, [1.0, 2.5, 4.0]);
}

fn compose_one_item(info: &PdfPageInfo, item: &PdfItem) -> PdfComposeInput {
    PdfComposeInput {
        pages: PdfPageInfos { ptr: info, len: 1 },
        items: PdfItems { ptr: item, len: 1 },
        ..PdfComposeInput::default()
    }
}

#[test]
fn compose_reconstructs_font_metadata_fields() {
    let storage = super::output::Storage::default();
    let item = storage.item(&sample_text_item(), 100.0);
    assert_eq!(
        super::output::parse_font_weight(item.font_weight).unwrap(),
        Some(700)
    );
    assert_eq!(
        super::output::parse_bold_source(item.bold_source).unwrap(),
        Some(pdf_inspector::BoldSource::FontName)
    );
    assert_eq!(
        super::output::parse_fixed_pitch(item.fixed_pitch).unwrap(),
        Some(true)
    );
    let info = PdfPageInfo {
        page: 1,
        width: 200.0,
        height: 100.0,
        rotation: 0,
    };
    let input = compose_one_item(&info, &item);
    let mut options = input::default_markdown();
    options.flags &= !PDF_MD_BOLD;
    let plain = compose_ok(&input, &options);
    assert!(!unsafe { string(plain.get().markdown) }.contains("**"));
    options.flags = u32::MAX;
    assert_eq!(compose_err(&input, &options), PDF_INVALID_ARGUMENT);
    let mut bad = item;
    bad.font_weight = 50;
    let input = compose_one_item(&info, &bad);
    assert_eq!(compose_err(&input, null()), PDF_INVALID_ARGUMENT);
}

#[test]
fn compose_accepts_dest_page_without_uri() {
    let storage = super::output::Storage::default();
    let item = PdfItem {
        page: 1,
        kind: PDF_ITEM_LINK,
        dest_page: 2,
        bounds: PdfBox {
            x0: 1.0,
            y0: 1.0,
            x1: 10.0,
            y1: 10.0,
        },
        text: storage.bytes(""),
        font: storage.bytes(""),
        font_tag: storage.bytes(""),
        link: storage.bytes(""),
        ..PdfItem::default()
    };
    let info = PdfPageInfo {
        page: 1,
        width: 200.0,
        height: 100.0,
        rotation: 0,
    };
    compose_ok(&compose_one_item(&info, &item), null());
    let mut missing = item;
    missing.dest_page = 0;
    assert_eq!(
        compose_err(&compose_one_item(&info, &missing), null()),
        PDF_INVALID_ARGUMENT
    );
}

#[test]
fn display_frame_matches_sheet_on_unrotated_pages() {
    let doc = with_rotate(0);
    let mut sheet_request = input::default_request();
    sheet_request.outputs = PDF_OUT_ITEMS | PDF_OUT_GEOMETRY;
    sheet_request.frame = PDF_FRAME_SHEET;
    let sheet = doc.run(&sheet_request);
    let mut display_request = input::default_request();
    display_request.outputs = PDF_OUT_ITEMS | PDF_OUT_GEOMETRY;
    display_request.frame = PDF_FRAME_DISPLAY;
    let display = doc.run(&display_request);
    let sheet_page = &sheet.pages()[0];
    let display_page = &display.pages()[0];
    assert_eq!(
        (display_page.info.width, display_page.info.height),
        (sheet_page.info.width, sheet_page.info.height)
    );
    let sheet_items = unsafe { input::slice(sheet_page.items.ptr, sheet_page.items.len).unwrap() };
    let display_items =
        unsafe { input::slice(display_page.items.ptr, display_page.items.len).unwrap() };
    assert_eq!(sheet_items.len(), display_items.len());
    for (a, b) in sheet_items.iter().zip(display_items.iter()) {
        assert_eq!(
            (
                a.bounds.x0,
                a.bounds.y0,
                a.bounds.x1,
                a.bounds.y1,
                a.rotation
            ),
            (
                b.bounds.x0,
                b.bounds.y0,
                b.bounds.x1,
                b.bounds.y1,
                b.rotation
            ),
            "{a:?} vs {b:?}"
        );
    }
    // A region queried in either frame selects the same text.
    let first = sheet_items
        .iter()
        .find(|i| unsafe { string(i.text) }.contains("Visible"))
        .unwrap();
    for frame in [PDF_FRAME_SHEET, PDF_FRAME_DISPLAY] {
        let mut r = input::default_request();
        r.frame = frame;
        let region = PdfRegionInput {
            page: 1,
            kind: PDF_REGION_TEXT,
            bounds: first.bounds,
        };
        r.regions = PdfRegionInputs {
            ptr: &region,
            len: 1,
        };
        let queried = doc.run(&r);
        let regions = unsafe { input::slice(queried.get().regions.ptr, 1).unwrap() };
        assert!(
            unsafe { string(regions[0].text) }.contains("Visible glyph"),
            "frame {frame}"
        );
    }
}

#[test]
fn display_frame_turns_with_inherited_rotate() {
    for (rotation, display_size) in [
        (0, (300.0, 400.0)),
        (90, (400.0, 300.0)),
        (180, (300.0, 400.0)),
        (270, (400.0, 300.0)),
    ] {
        let doc = with_rotate(rotation);
        let mut sheet_request = input::default_request();
        sheet_request.outputs = PDF_OUT_ITEMS | PDF_OUT_GEOMETRY;
        sheet_request.frame = PDF_FRAME_SHEET;
        let sheet = doc.run(&sheet_request);
        let sheet_page = &sheet.pages()[0];
        assert_eq!(
            (
                sheet_page.info.width,
                sheet_page.info.height,
                sheet_page.info.rotation
            ),
            (300.0, 400.0, rotation as u32)
        );
        let mut display_request = input::default_request();
        display_request.outputs = PDF_OUT_ITEMS | PDF_OUT_GEOMETRY;
        display_request.frame = PDF_FRAME_DISPLAY;
        let display = doc.run(&display_request);
        let display_page = &display.pages()[0];
        assert_eq!(
            (
                display_page.info.width,
                display_page.info.height,
                display_page.info.rotation
            ),
            (display_size.0, display_size.1, rotation as u32),
            "rotation {rotation}"
        );
        let sheet_items =
            unsafe { input::slice(sheet_page.items.ptr, sheet_page.items.len).unwrap() };
        let display_items =
            unsafe { input::slice(display_page.items.ptr, display_page.items.len).unwrap() };
        assert_eq!(sheet_items.len(), display_items.len());
        let sheet_item = sheet_items
            .iter()
            .find(|i| unsafe { string(i.text) }.contains("Visible"))
            .unwrap();
        let display_item = display_items
            .iter()
            .find(|i| unsafe { string(i.text) }.contains("Visible"))
            .unwrap();
        assert_eq!(
            display_item.rotation,
            (sheet_item.rotation + rotation as f32).rem_euclid(360.0),
            "rotation {rotation}: {sheet_item:?} vs {display_item:?}"
        );
        // Display bounds stay inside the rendered page.
        assert!(
            display_item.bounds.x0 >= 0.0
                && display_item.bounds.y0 >= 0.0
                && display_item.bounds.x1 <= display_size.0 + 0.01
                && display_item.bounds.y1 <= display_size.1 + 0.01,
            "rotation {rotation}: {:?} not in {:?}",
            display_item.bounds,
            display_size
        );
        // The display rect selects the text in the display frame.
        let region = PdfRegionInput {
            page: 1,
            kind: PDF_REGION_TEXT,
            bounds: display_item.bounds,
        };
        let mut r = input::default_request();
        r.frame = PDF_FRAME_DISPLAY;
        r.regions = PdfRegionInputs {
            ptr: &region,
            len: 1,
        };
        let queried = doc.run(&r);
        let regions = unsafe { input::slice(queried.get().regions.ptr, 1).unwrap() };
        assert!(
            unsafe { string(regions[0].text) }.contains("Visible glyph"),
            "rotation {rotation}"
        );
        if rotation == 90 || rotation == 270 {
            // The same rect read as sheet coordinates lands on empty paper.
            let mut r = input::default_request();
            r.frame = PDF_FRAME_SHEET;
            r.regions = PdfRegionInputs {
                ptr: &region,
                len: 1,
            };
            let queried = doc.run(&r);
            let regions = unsafe { input::slice(queried.get().regions.ptr, 1).unwrap() };
            assert!(
                !unsafe { string(regions[0].text) }.contains("Visible glyph"),
                "rotation {rotation}: display rect should miss in sheet frame"
            );
        }
    }
}

#[test]
fn external_ocr_needs_no_bitmap_or_native_backend() {
    let doc = Doc::open("scan_with_native_header_text");
    let span = PdfOcrSpan {
        text: view(b"External recognition provides the missing document content."),
        polygon: PdfQuad {
            points: [
                PdfPoint { x: 40.0, y: 120.0 },
                PdfPoint { x: 400.0, y: 120.0 },
                PdfPoint { x: 400.0, y: 140.0 },
                PdfPoint { x: 40.0, y: 140.0 },
            ],
        },
        confidence: 0.99,
        ..PdfOcrSpan::default()
    };
    let external = PdfOcrPageInput {
        page: 1,
        flags: PDF_HAS_CONFIDENCE,
        confidence: 0.99,
        model: view(b"external-test"),
        model_revision: view(b"fixture"),
        spans: PdfOcrSpans { ptr: &span, len: 1 },
        ..PdfOcrPageInput::default()
    };
    let mut r = input::default_request();
    r.external_ocr = PdfOcrPageInputs {
        ptr: &external,
        len: 1,
    };
    let result = doc.run(&r);
    assert!(unsafe { string(result.get().markdown) }.contains("External recognition"));
    let p = &result.pages()[0];
    assert!(unsafe { string(p.markdown) }.contains("External recognition"));
    assert_eq!(p.flags & PDF_PAGE_OCR_RAN, PDF_PAGE_OCR_RAN);
    assert_eq!(p.provenance.render_dpi, 0.0);
    assert_eq!(unsafe { string(p.provenance.model) }, "external-test");
    assert_eq!(result.get().present & PDF_OUT_ANALYSIS, PDF_OUT_ANALYSIS);
}

#[test]
fn region_batch_order_and_tsr_validation() {
    let doc = Doc::open("bare_name_struct");
    let mut r = input::default_request();
    let bounds = PdfBox {
        x0: 0.0,
        y0: 0.0,
        x1: 500.0,
        y1: 700.0,
    };
    let regions = [
        PdfRegionInput {
            page: 1,
            kind: PDF_REGION_TEXT,
            bounds,
        },
        PdfRegionInput {
            page: 1,
            kind: PDF_REGION_TABLE,
            bounds,
        },
        PdfRegionInput {
            page: 1,
            kind: PDF_REGION_GRID,
            bounds,
        },
    ];
    r.regions = PdfRegionInputs {
        ptr: regions.as_ptr(),
        len: 3,
    };
    let result = doc.run(&r);
    let out = unsafe { input::slice(result.get().regions.ptr, 3).unwrap() };
    assert_eq!(
        out.iter().map(|r| r.kind).collect::<Vec<_>>(),
        vec![PDF_REGION_TEXT, PDF_REGION_TABLE, PDF_REGION_GRID]
    );
    assert!(unsafe { string(out[0].text) }.contains("Test"));
    let tokens = [
        view(b"<table>"),
        view(b"<tr>"),
        view(b"<td></td>"),
        view(b"</tr>"),
        view(b"</table>"),
    ];
    let quad = PdfQuad {
        points: [
            PdfPoint { x: 0.0, y: 0.0 },
            PdfPoint { x: 500.0, y: 0.0 },
            PdfPoint { x: 500.0, y: 700.0 },
            PdfPoint { x: 0.0, y: 700.0 },
        ],
    };
    let mut table = PdfTableInput {
        page: 1,
        mode: PDF_TSR_STRICT,
        bounds,
        tokens: PdfStrings {
            ptr: tokens.as_ptr(),
            len: tokens.len(),
        },
        cells: PdfQuads { ptr: &quad, len: 1 },
    };
    r.tables = PdfTableInputs {
        ptr: &table,
        len: 1,
    };
    let result = doc.run(&r);
    let table_out = unsafe { &*result.get().tables.ptr };
    assert_eq!(table_out.flags, PDF_TABLE_FROM_HINT | PDF_TABLE_HAS_BOUNDS);
    assert_eq!(table_out.kind, PDF_TABLE_DATA);
    assert_eq!(result.get().present & PDF_OUT_TABLES, PDF_OUT_TABLES);
    assert_eq!(table_out.cells.len, 1);
    assert_eq!(
        unsafe { (*table_out.cells.ptr).flags } & (PDF_CELL_HAS_BOUNDS | PDF_CELL_SPAN_KNOWN),
        PDF_CELL_HAS_BOUNDS | PDF_CELL_SPAN_KNOWN
    );
    assert!(unsafe { string((*table_out.cells.ptr).text) }.contains("Test"));
    table.cells.len = 0;
    r.tables.ptr = &table;
    unsafe {
        let e = expect_failure(doc.0, &r, PDF_INVALID_ARGUMENT);
        pdf_inspector_error_free(e);
    }
    for span in [
        &b" colspan=\"400\""[..],
        b" rowspan=\"2\"",
        b" rowspan=\"abc\"",
        b"rowspan",
    ] {
        let tokens = [
            view(b"<table>"),
            view(b"<tr>"),
            view(b"<td"),
            view(span),
            view(b">"),
            view(b"</td>"),
            view(b"</tr>"),
            view(b"</table>"),
        ];
        table.tokens = PdfStrings {
            ptr: tokens.as_ptr(),
            len: tokens.len(),
        };
        table.cells.len = 1;
        r.tables.ptr = &table;
        unsafe {
            let e = expect_failure(doc.0, &r, PDF_INVALID_ARGUMENT);
            assert!(string((*e).message).contains("span"));
            pdf_inspector_error_free(e);
        }
    }
    let tokens = [
        view(b"<table>"),
        view(b"<tr>"),
        view(b"<td"),
        view(b" colspan=\"2\""),
        view(b">"),
        view(b"</td>"),
        view(b"</tr>"),
        view(b"</table>"),
    ];
    table.tokens = PdfStrings {
        ptr: tokens.as_ptr(),
        len: tokens.len(),
    };
    table.mode = PDF_TSR_AUTO;
    r.tables.ptr = &table;
    let result = doc.run(&r);
    let table_out = unsafe { &*result.get().tables.ptr };
    assert_eq!(table_out.input_index, 0);
    assert!(!unsafe { string(table_out.markdown) }.is_empty());
}

#[test]
fn unavailable_capabilities_fail_explicitly() {
    let doc = Doc::open("bare_name_struct");
    for (available, outputs, mode) in [
        (cfg!(feature = "render-pdfium"), PDF_OUT_RENDER, PDF_OCR_OFF),
        (cfg!(feature = "ocr"), PDF_OUT_MARKDOWN, PDF_OCR_FORCE),
    ] {
        if !available {
            let mut r = input::default_request();
            r.outputs = outputs;
            r.ocr.mode = mode;
            unsafe {
                let e = expect_failure(doc.0, &r, PDF_UNSUPPORTED);
                pdf_inspector_error_free(e);
            }
        }
    }
}

#[test]
fn panic_is_reported_without_publishing_result() {
    unsafe {
        let mut out: *mut PdfResult = std::ptr::dangling_mut();
        let mut error = null_mut();
        let status =
            publish::<ResultOwner>(&mut out, &mut error, || panic!("intentional test panic"));
        assert_eq!(status, PDF_PANIC);
        assert!(out.is_null());
        assert_eq!((*error).status, PDF_PANIC);
        pdf_inspector_error_free(error);
    }
}

#[cfg(feature = "render-pdfium")]
#[test]
fn render_runtime_coordinates_and_reuse() {
    if std::env::var_os("PDFIUM_LIB_PATH").is_none() {
        eprintln!("PDFIUM_LIB_PATH absent; native renderer smoke not provisioned");
        return;
    }
    let doc = Doc::open("cropbox_offset_origin");
    unsafe {
        let options = PdfRuntimeOptions {
            capabilities: PDF_CAP_RENDER,
            ..runtime::defaults()
        };
        let mut info = PdfRuntimeInfo::default();
        let mut error = null_mut();
        assert_success(
            pdf_inspector_prepare_runtime(&options, &mut info, &mut error),
            error,
        );
        assert_eq!(info.capabilities, PDF_CAP_RENDER);
        assert!(info.model.ptr.is_null());
    }
    let mut r = input::default_request();
    r.outputs = PDF_OUT_RENDER | PDF_OUT_ITEMS;
    r.render.dpi = 96.0;
    let result = doc.run(&r);
    let image = result.pages()[0].image;
    assert_eq!(image.pixels.len, image.stride * image.height as usize);
    let point = image.pixel_to_page.apply(50.0, 75.0);
    let round = image.page_to_pixel.apply(point.0, point.1);
    assert!((round.0 - 50.0).abs() < 1e-5 && (round.1 - 75.0).abs() < 1e-5);
    let other = doc.run(&r);
    drop(doc);
    assert_eq!(image.pixels.len, other.pages()[0].image.pixels.len);
    assert!(!unsafe { input::bytes(image.pixels).unwrap() }.is_empty());
}

#[cfg(feature = "ocr")]
#[test]
fn native_ocr_runtime_returns_recognition_and_provenance() {
    let Some(models) = std::env::var_os("PDF_INSPECTOR_OCR_TEST_MODELS") else {
        eprintln!("OCR runtime models not provisioned");
        return;
    };
    let models = models.to_str().unwrap();
    unsafe {
        let options = PdfRuntimeOptions {
            model_directory: view(models.as_bytes()),
            ..runtime::defaults()
        };
        let mut info = PdfRuntimeInfo::default();
        let mut error = null_mut();
        assert_success(
            pdf_inspector_prepare_runtime(&options, &mut info, &mut error),
            error,
        );
        assert_eq!(info.capabilities, PDF_CAP_RENDER | PDF_CAP_OCR);
        assert!(!string(info.model).is_empty());
    }
    let doc = Doc::open("scan_with_native_header_text");
    let mut r = input::default_request();
    r.outputs |= PDF_OUT_ANALYSIS;
    r.ocr.mode = PDF_OCR_FORCE;
    r.ocr.download_policy = PDF_DOWNLOAD_OFFLINE;
    r.ocr.model_directory = view(models.as_bytes());
    let result = doc.run(&r);
    assert!(!unsafe { string(result.get().markdown) }.trim().is_empty());
    let p = &result.pages()[0];
    assert!(!unsafe { string(p.markdown) }.trim().is_empty());
    assert_eq!(p.flags & PDF_PAGE_OCR_RAN, PDF_PAGE_OCR_RAN);
    assert_eq!(p.provenance.source, PDF_CONTENT_OCR);
    assert!(p.provenance.render_dpi > 0.0);
}

#[test]
fn emit_c_layout_contract() {
    macro_rules! record {
  ($t:ty; $($field:ident),* $(,)?) => {
   println!("PDF_LAYOUT: _Static_assert(sizeof({}) == {}, \"size of {}\");",stringify!($t),std::mem::size_of::<$t>(),stringify!($t));
   println!("PDF_LAYOUT: _Static_assert(_Alignof({}) == {}, \"alignment of {}\");",stringify!($t),std::mem::align_of::<$t>(),stringify!($t));
   $(println!("PDF_LAYOUT: _Static_assert(offsetof({}, {}) == {}, \"offset of {}.{}\");",stringify!($t),stringify!($field),std::mem::offset_of!($t,$field),stringify!($t),stringify!($field));)*
  };
 }
    record!(PdfBytes; ptr, len);
    record!(PdfSource; kind, data, password);
    record!(PdfBox; x0, y0, x1, y1);
    record!(PdfPoint; x, y);
    record!(PdfTransform; a, b, c, d, e, f);
    record!(PdfMarkdownOptions; flags, profile, base_font_size);
    record!(PdfDetectionOptions; strategy, sample_size, min_text_ops, text_page_ratio, pages);
    record!(PdfRenderOptions; dpi, format, flags, max_page_bytes);
    record!(PdfOcrOptions; mode, download_policy, minimum_confidence, hosted_confidence, model_directory);
    record!(PdfRuntimeOptions; capabilities, download_policy, model_directory);
    record!(PdfRuntimeInfo; capabilities, model, model_revision);
    record!(PdfRegionInput; page, kind, bounds);
    record!(PdfQuad; points);
    record!(PdfTableInput; page, mode, bounds, tokens, cells);
    record!(PdfOcrSpan; text, polygon, confidence, orientation, flags);
    record!(PdfOcrPageInput; page, flags, confidence, processing_ms, model, model_revision, warnings, spans);
    record!(PdfRequest; outputs, pages, markdown, detection, render, ocr, regions, tables, external_ocr, frame, flags, bold_weight_threshold);
    record!(PdfItem; page, kind, flags, bounds, font_size, rotation, baseline_shift, mcid, font_weight, bold_source, fixed_pitch, dest_page, text, font, font_tag, link);
    record!(PdfStructureElement; page, mcid, role);
    record!(PdfStructureNode; parent, role, alt_text, actual_text, language, references);
    record!(PdfContentReference; page, mcid);
    record!(PdfRectangle; page, bounds);
    record!(PdfSegment; page, start, end);
    record!(PdfPageInfo; page, width, height, rotation);
    record!(PdfComposeInput; pages, items, rectangles, lines, structure);
    record!(PdfImage; width, height, stride, format, pixels, pixel_to_page, page_to_pixel);
    record!(PdfProvenance; source, flags, confidence, render_dpi, render_ms, ocr_ms, assembly_ms, model, model_revision, warnings);
    record!(PdfPageQuality; alphanumeric_chars, visible_chars, density, replacement_chars, longest_replacement_run, english_cosine, score);
    record!(PdfInterval; x0, x1);
    record!(PdfLoadAudit; flags, leading_bytes, widened_form_bboxes);
    record!(PdfPage; info, flags, reading_order, text_orientation, quality, markdown, text, ocr_reasons, items, columns, charts, image_regions, structure, rectangles, lines, image, provenance);
    record!(PdfCell; row, column, row_span, column_span, flags, bounds, text);
    record!(PdfRegion; page, kind, flags, bounds, text, ocr_reason, tokens, cells);
    record!(PdfTable; page, flags, input_index, kind, bounds, markdown, fallback_reason, column_edges, row_edges, cells);
    record!(PdfResult; present, pdf_type, page_count, confidence, flags, pages_sampled, pages_with_text, processing_ms, title, markdown, text, pages, regions, tables, structure_nodes);
    record!(PdfError; status, message);
    record!(PdfDocumentInfo; page_count, audit, pages);
    record!(PdfPageNumbers; ptr, len);
    record!(PdfStrings; ptr, len);
    record!(PdfQuads; ptr, len);
    record!(PdfBoxes; ptr, len);
    record!(PdfIntervals; ptr, len);
    record!(PdfFloats; ptr, len);
    record!(PdfItems; ptr, len);
    record!(PdfPageInfos; ptr, len);
    record!(PdfStructureElements; ptr, len);
    record!(PdfRectangles; ptr, len);
    record!(PdfSegments; ptr, len);
    record!(PdfRegionInputs; ptr, len);
    record!(PdfTableInputs; ptr, len);
    record!(PdfOcrSpans; ptr, len);
    record!(PdfOcrPageInputs; ptr, len);
    record!(PdfPages; ptr, len);
    record!(PdfRegions; ptr, len);
    record!(PdfTables; ptr, len);
    record!(PdfCells; ptr, len);
    record!(PdfStructureNodes; ptr, len);
    record!(PdfContentReferences; ptr, len);
}
