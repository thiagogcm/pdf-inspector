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
            };
            let options = PdfOpenOptions {
                password: password.map_or(PdfBytes::default(), |p| view(p.as_bytes())),
            };
            let status = pdf_inspector_open(&source, &options, &mut doc, &mut err);
            assert_success(status, err);
            Self(doc)
        }
    }
    fn run(&self, r: &PdfRequest) -> ResultOwner {
        unsafe {
            let mut out = null_mut();
            let mut e = null_mut();
            let status = pdf_inspector_execute(self.0, r, &mut out, &mut e);
            assert_success(status, e);
            ResultOwner(out)
        }
    }
}
impl Drop for Doc {
    fn drop(&mut self) {
        unsafe { pdf_inspector_document_free(self.0) }
    }
}
struct ResultOwner(*mut PdfResult);
impl ResultOwner {
    fn get(&self) -> &PdfResultView {
        unsafe { &*pdf_inspector_result_view(self.0) }
    }
    fn pages(&self) -> &[PdfPage] {
        unsafe { input::slice(self.get().pages.ptr, self.get().pages.len).unwrap() }
    }
}
impl Drop for ResultOwner {
    fn drop(&mut self) {
        unsafe { pdf_inspector_result_free(self.0) }
    }
}
unsafe fn assert_success(status: i32, error: *mut PdfErrorHandle) {
    let message = if error.is_null() {
        String::new()
    } else {
        string((*pdf_inspector_error_view(error)).message)
    };
    pdf_inspector_error_free(error);
    assert_eq!(status, PDF_OK, "{message}");
}
unsafe fn expect_failure(
    doc: *const PdfDocument,
    r: &PdfRequest,
    expected: i32,
) -> *mut PdfErrorHandle {
    let mut result = std::ptr::dangling_mut();
    let mut error = null_mut();
    assert_eq!(
        pdf_inspector_execute(doc, r, &mut result, &mut error),
        expected
    );
    assert!(result.is_null());
    assert!(!error.is_null());
    assert_eq!((*pdf_inspector_error_view(error)).status, expected);
    error
}

#[test]
fn owned_source_reusable_document_and_independent_snapshots() {
    let doc = Doc::open("bare_name_struct");
    let mut r = input::default_request();
    r.outputs |= PDF_ITEMS | PDF_STRUCTURE | PDF_TEXT | PDF_GEOMETRY;
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
    };
    let mut doc = null_mut();
    let mut err = null_mut();
    unsafe {
        assert_success(pdf_inspector_open(&source, null(), &mut doc, &mut err), err);
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
    r.outputs = PDF_INSPECTION;
    let result = doc.run(&r);
    assert!(result.get().markdown.ptr.is_null());
    assert_eq!(result.get().present & PDF_ANALYSIS, 0);
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
        let message = string((*pdf_inspector_error_view(first)).message);
        let address = doc.0 as usize;
        let barrier = Arc::new(Barrier::new(2));
        let other = barrier.clone();
        let thread = std::thread::spawn(move || {
            let mut b = input::default_request();
            b.ocr.minimum_confidence = f32::NAN;
            let e = expect_failure(address as *const PdfDocument, &b, PDF_INVALID_ARGUMENT);
            other.wait();
            let message = string((*pdf_inspector_error_view(e)).message);
            pdf_inspector_error_free(e);
            message
        });
        barrier.wait();
        let second = thread.join().unwrap();
        assert_ne!(message, second);
        let _result = doc.run(&input::default_request());
        assert_eq!(message, string((*pdf_inspector_error_view(first)).message));
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
                r.outputs |= PDF_ITEMS | PDF_STRUCTURE | PDF_TABLES;
                assert_success(
                    pdf_inspector_execute(address as *const PdfDocument, &r, &mut out, &mut err),
                    err,
                );
                let text = string((*pdf_inspector_result_view(out)).markdown);
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
    r.outputs = PDF_RUNTIME;
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
    };
    unsafe {
        let mut out = std::ptr::dangling_mut();
        let mut e = null_mut();
        assert_eq!(
            pdf_inspector_open(&source, null(), &mut out, &mut e),
            PDF_PASSWORD_ERROR
        );
        assert!(out.is_null());
        pdf_inspector_error_free(e);
        assert_eq!(
            pdf_inspector_open(null(), null(), &mut out, &mut e),
            PDF_INVALID_ARGUMENT
        );
        assert!(out.is_null());
        pdf_inspector_error_free(e);
    }
    let doc = Doc::bytes(&bytes, Some("secret123"));
    let mut r = input::default_request();
    r.outputs |= PDF_ITEMS | PDF_STRUCTURE | PDF_GEOMETRY | PDF_TEXT;
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
        let mut c = PdfComposeInput::default();
        pdf_inspector_compose_init(&mut c);
        c.text = view(text);
        let mut out = null_mut();
        let mut e = null_mut();
        assert_success(pdf_inspector_compose(&c, &mut out, &mut e), e);
        let result = ResultOwner(out);
        assert!(string(result.get().markdown).contains("plain\0text"));
        c.text = view(&[255]);
        assert_eq!(
            pdf_inspector_compose(&c, &mut out, &mut e),
            PDF_INVALID_ARGUMENT
        );
        assert!(out.is_null());
        pdf_inspector_error_free(e);
        c.text = view(b"");
        assert_success(pdf_inspector_compose(&c, &mut out, &mut e), e);
        let empty = ResultOwner(out);
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
        r.outputs |= PDF_ITEMS | PDF_STRUCTURE | PDF_GEOMETRY;
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
            kind: PDF_COMPOSE_ITEMS,
            pages: PdfPageInfos {
                ptr: infos.as_ptr(),
                len: infos.len(),
            },
            items: PdfItems {
                ptr: all.as_ptr(),
                len: all.len(),
            },
            markdown: input::default_markdown(),
            ..PdfComposeInput::default()
        };
        let mut out = null_mut();
        let mut e = null_mut();
        unsafe {
            assert_success(pdf_inspector_compose(&c, &mut out, &mut e), e);
        }
        let composed = ResultOwner(out);
        assert!(unsafe { string(composed.get().markdown) }.contains(expected.trim()));
        c.items = PdfItems {
            ptr: null(),
            len: 1,
        };
        unsafe {
            assert_eq!(
                pdf_inspector_compose(&c, &mut out, &mut e),
                PDF_INVALID_ARGUMENT
            );
            pdf_inspector_error_free(e);
        }
    }
}

fn symbol_rewrite_pdf() -> Vec<u8> {
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
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn legacy_symbol_rewrite_is_item_flag_and_survives_composition() {
    let bytes = symbol_rewrite_pdf();
    let doc = Doc::bytes(&bytes, None);
    let mut r = input::default_request();
    r.outputs = PDF_ITEMS;
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
    let compose = PdfComposeInput {
        kind: PDF_COMPOSE_ITEMS,
        pages: PdfPageInfos { ptr: &info, len: 1 },
        items: PdfItems {
            ptr: items.as_ptr(),
            len: items.len(),
        },
        markdown: input::default_markdown(),
        ..PdfComposeInput::default()
    };
    let mut out = null_mut();
    let mut e = null_mut();
    unsafe {
        assert_success(pdf_inspector_compose(&compose, &mut out, &mut e), e);
    }
    drop(ResultOwner(out));
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
            let mut bytes = Vec::new();
            source.save_to(&mut bytes).unwrap();
            let doc = Doc::bytes(&bytes, None);
            let mut r = input::default_request();
            r.outputs = PDF_ITEMS | PDF_TEXT;
            #[cfg(feature = "render-pdfium")]
            if std::env::var_os("PDFIUM_LIB_PATH").is_some() {
                r.outputs |= PDF_RENDER;
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
            if r.outputs & PDF_RENDER != 0 {
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

fn bytes_with_rotate(rotation: i32) -> Vec<u8> {
    let mut source = lopdf::Document::load("../tests/fixtures/cropbox_offset_origin.pdf").unwrap();
    source
        .get_object_mut((2, 0))
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Rotate", rotation);
    let mut bytes = Vec::new();
    source.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn request_init_defaults_to_sheet_frame() {
    let mut request = PdfRequest {
        outputs: 0xdead_beef,
        frame: 0xdead_beef,
        ..PdfRequest::default()
    };
    unsafe {
        assert_success(pdf_inspector_request_init(&mut request), null_mut());
    }
    assert_eq!(request.frame, PDF_FRAME_SHEET);
}

#[test]
fn unknown_frame_is_rejected() {
    let bytes = bytes_with_rotate(0);
    let doc = Doc::bytes(&bytes, None);
    let mut r = input::default_request();
    r.outputs = PDF_ITEMS;
    r.frame = 99;
    unsafe {
        let error = expect_failure(doc.0, &r, PDF_INVALID_ARGUMENT);
        pdf_inspector_error_free(error);
    }
}

#[test]
fn display_frame_matches_sheet_on_unrotated_pages() {
    let bytes = bytes_with_rotate(0);
    let doc = Doc::bytes(&bytes, None);
    let mut sheet_request = input::default_request();
    sheet_request.outputs = PDF_ITEMS | PDF_GEOMETRY;
    sheet_request.frame = PDF_FRAME_SHEET;
    let sheet = doc.run(&sheet_request);
    let mut display_request = input::default_request();
    display_request.outputs = PDF_ITEMS | PDF_GEOMETRY;
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
        let bytes = bytes_with_rotate(rotation);
        let doc = Doc::bytes(&bytes, None);
        let mut sheet_request = input::default_request();
        sheet_request.outputs = PDF_ITEMS | PDF_GEOMETRY;
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
        display_request.outputs = PDF_ITEMS | PDF_GEOMETRY;
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
    assert_eq!(result.get().present & PDF_ANALYSIS, PDF_ANALYSIS);
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
    assert_eq!(result.get().present & PDF_TABLES, PDF_TABLES);
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
            assert!(string((*pdf_inspector_error_view(e)).message).contains("span"));
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
        (cfg!(feature = "render-pdfium"), PDF_RENDER, PDF_OCR_OFF),
        (cfg!(feature = "ocr"), PDF_MARKDOWN, PDF_OCR_FORCE),
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
        let mut out: *mut u32 = std::ptr::dangling_mut();
        let mut error = null_mut();
        let status = publish(&mut out, &mut error, || panic!("intentional test panic"));
        assert_eq!(status, PDF_PANIC);
        assert!(out.is_null());
        assert_eq!((*pdf_inspector_error_view(error)).status, PDF_PANIC);
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
        let mut result = null_mut();
        let mut error = null_mut();
        assert_success(
            pdf_inspector_prepare_runtime(&options, &mut result, &mut error),
            error,
        );
        let result = ResultOwner(result);
        assert_eq!(result.get().present, PDF_RUNTIME);
        assert_eq!(result.get().runtime.ready, PDF_CAP_RENDER);
    }
    let mut r = input::default_request();
    r.outputs = PDF_RENDER | PDF_ITEMS;
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
        let mut result = null_mut();
        let mut error = null_mut();
        assert_success(
            pdf_inspector_prepare_runtime(&options, &mut result, &mut error),
            error,
        );
        let result = ResultOwner(result);
        assert_eq!(result.get().runtime.ready, PDF_CAP_RENDER | PDF_CAP_OCR);
        assert!(!string(result.get().runtime.model).is_empty());
    }
    let doc = Doc::open("scan_with_native_header_text");
    let mut r = input::default_request();
    r.outputs |= PDF_ANALYSIS;
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
    record!(PdfSource; kind, data);
    record!(PdfOpenOptions; password);
    record!(PdfBox; x0, y0, x1, y1);
    record!(PdfPoint; x, y);
    record!(PdfTransform; a, b, c, d, e, f);
    record!(PdfMarkdownOptions; flags, profile, base_font_size);
    record!(PdfDetectionOptions; strategy, sample_size, min_text_ops, text_page_ratio, pages);
    record!(PdfRenderOptions; dpi, format, annotations, form_fields, max_page_bytes);
    record!(PdfOcrOptions; mode, download_policy, minimum_confidence, hosted_confidence, model_directory);
    record!(PdfRuntimeOptions; capabilities, download_policy, model_directory);
    record!(PdfRuntimeInfo; ready, model, model_revision);
    record!(PdfRegionInput; page, kind, bounds);
    record!(PdfQuad; points);
    record!(PdfTableInput; page, mode, bounds, tokens, cells);
    record!(PdfOcrSpan; text, polygon, confidence, orientation, flags);
    record!(PdfOcrPageInput; page, flags, confidence, processing_ms, model, model_revision, warnings, spans);
    record!(PdfRequest; outputs, pages, markdown, detection, render, ocr, regions, tables, external_ocr, frame);
    record!(PdfItem; page, kind, flags, bounds, font_size, rotation, baseline_shift, mcid, text, font, font_tag, link);
    record!(PdfStructureElement; page, mcid, role);
    record!(PdfStructureNode; id, parent, role, alt_text, actual_text, language, references);
    record!(PdfContentReference; page, mcid);
    record!(PdfRectangle; page, bounds);
    record!(PdfSegment; page, start, end);
    record!(PdfPageInfo; page, width, height, rotation);
    record!(PdfComposeInput; kind, text, pages, items, rectangles, lines, structure, markdown);
    record!(PdfImage; width, height, stride, format, pixels, pixel_to_page, page_to_pixel);
    record!(PdfProvenance; source, flags, confidence, render_dpi, render_ms, ocr_ms, assembly_ms, model, model_revision, warnings);
    record!(PdfPage; info, flags, markdown, text, ocr_reasons, items, structure, rectangles, lines, image, provenance);
    record!(PdfCell; row, column, row_span, column_span, flags, bounds, text);
    record!(PdfRegion; page, kind, flags, bounds, text, ocr_reason, tokens, cells);
    record!(PdfTable; page, flags, input_index, bounds, markdown, fallback_reason, cells);
    record!(PdfResultView; present, pdf_type, page_count, confidence, has_encoding_issues, processing_ms, title, markdown, text, pages, regions, tables, structure_nodes, runtime);
    record!(PdfDiagnostic; status, message);
    record!(PdfPageNumbers; ptr, len);
    record!(PdfStrings; ptr, len);
    record!(PdfQuads; ptr, len);
    record!(PdfBoxes; ptr, len);
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
