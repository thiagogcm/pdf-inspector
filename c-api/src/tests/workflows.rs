use super::*;
use lopdf::{dictionary, Document, Object, Stream};

fn document(contents: &[&str], image: bool) -> Document {
    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let font = doc.add_object(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
    );
    let mut resources = dictionary! { "Font" => dictionary! { "F1" => font } };
    if image {
        let raster = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => 64, "Height" => 64,
                "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
            },
            vec![255; 64 * 64],
        ));
        resources.set("XObject", dictionary! { "Im1" => raster });
    }
    let kids: Vec<Object> = contents
        .iter()
        .map(|content| {
            let stream = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
            doc.add_object(dictionary! {
                "Type" => "Page", "Parent" => pages, "Contents" => stream,
                "Resources" => resources.clone(),
                "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
            })
            .into()
        })
        .collect();
    doc.objects.insert(
        pages,
        dictionary! { "Type" => "Pages", "Count" => kids.len() as i64, "Kids" => kids }.into(),
    );
    let root = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", root);
    doc
}
fn open(mut doc: Document) -> Doc {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    Doc::bytes(&bytes, None)
}
fn ocr_span(text: &'static str, x: f32, y: f32, width: f32) -> PdfOcrSpan {
    PdfOcrSpan {
        text: view(text.as_bytes()),
        confidence: 0.95,
        polygon: PdfQuad {
            points: [
                PdfPoint { x, y },
                PdfPoint { x: x + width, y },
                PdfPoint {
                    x: x + width,
                    y: y + 10.0,
                },
                PdfPoint { x, y: y + 10.0 },
            ],
        },
        ..PdfOcrSpan::default()
    }
}
fn recognition(spans: &[PdfOcrSpan]) -> PdfOcrPageInput {
    PdfOcrPageInput {
        page: 1,
        flags: PDF_HAS_CONFIDENCE,
        confidence: 0.95,
        model: view(b"external-test"),
        model_revision: view(b"1"),
        spans: PdfOcrSpans {
            ptr: spans.as_ptr(),
            len: spans.len(),
        },
        ..PdfOcrPageInput::default()
    }
}
fn attach(r: &mut PdfRequest, ocr: &PdfOcrPageInput) {
    r.external_ocr = PdfOcrPageInputs { ptr: ocr, len: 1 };
}

#[test]
fn analysis_and_detected_tables_need_no_markdown_or_hints() {
    let doc = Doc::open("real-estate-pricing");
    let mut r = input::default_request();
    r.outputs = PDF_ANALYSIS | PDF_TABLES;
    let result = doc.run(&r);
    assert!(result.get().markdown.ptr.is_null());
    assert!(result.pages().iter().all(|p| p.markdown.ptr.is_null()));
    assert_eq!(result.get().present & PDF_ANALYSIS, PDF_ANALYSIS);
    assert!(result
        .pages()
        .iter()
        .any(|p| p.flags & PDF_PAGE_HAS_TABLES != 0));
    assert!(result
        .pages()
        .iter()
        .any(|p| p.flags & PDF_PAGE_HAS_COLUMNS != 0));
    let tables = unsafe { input::slice(result.get().tables.ptr, result.get().tables.len).unwrap() };
    assert!(
        !tables.is_empty(),
        "automatic tables must not require TSR inputs"
    );
    assert!(tables
        .iter()
        .all(|t| t.flags & (PDF_TABLE_FROM_HINT | PDF_TABLE_HAS_BOUNDS) == 0));
    assert!(tables
        .iter()
        .any(|t| unsafe { string(t.markdown) }.contains("Multifamily")));
    let cells: Vec<_> = tables
        .iter()
        .flat_map(|t| unsafe { input::slice(t.cells.ptr, t.cells.len).unwrap() })
        .collect();
    assert!(cells
        .iter()
        .any(|c| unsafe { string(c.text) }.contains("0.937")));
    assert!(cells
        .iter()
        .all(|c| c.flags & (PDF_CELL_HAS_BOUNDS | PDF_CELL_SPAN_KNOWN) == 0));
    let selected = [tables[0].page];
    r.pages = PdfPageNumbers {
        ptr: selected.as_ptr(),
        len: 1,
    };
    let filtered = doc.run(&r);
    assert!(
        unsafe { input::slice(filtered.get().tables.ptr, filtered.get().tables.len).unwrap() }
            .iter()
            .all(|t| t.page == selected[0])
    );
    drop(doc);
    assert!(unsafe { string(tables[0].markdown) }.contains('|'));
}

#[test]
fn analysis_reports_garbled_text_without_text_output() {
    let doc = Doc::open("shifted_cipher_tounicode");
    let mut r = input::default_request();
    r.outputs = PDF_ANALYSIS;
    let result = doc.run(&r);
    assert_eq!(result.get().has_encoding_issues, 1);
    assert!(result
        .pages()
        .iter()
        .any(|p| p.flags & PDF_PAGE_ENCODING_ISSUES != 0));
    assert!(result
        .pages()
        .iter()
        .all(|p| p.items.ptr.is_null() && p.text.ptr.is_null()));
}

#[test]
fn semantic_nodes_keep_ancestors_metadata_and_selected_page_references() {
    let mut source = document(&["", ""], false);
    let pages: Vec<_> = source.get_pages().into_values().collect();
    let root = source.new_object_id();
    let section = source.new_object_id();
    let first = source.add_object(dictionary! { "Type" => "StructElem", "S" => "P", "P" => section, "Pg" => pages[0], "K" => 3 });
    let second = source.add_object(dictionary! {
        "Type" => "StructElem", "S" => "Figure", "P" => section, "Pg" => pages[1], "K" => 7,
        "Alt" => Object::string_literal("Revenue chart"), "ActualText" => Object::string_literal("Revenue rose"),
        "Lang" => Object::string_literal("en-US"),
    });
    source.objects.insert(section, dictionary! { "Type" => "StructElem", "S" => "Sect", "P" => root, "K" => vec![first.into(), second.into()] }.into());
    source.objects.insert(
        root,
        dictionary! { "Type" => "StructTreeRoot", "K" => vec![section.into()] }.into(),
    );
    source.catalog_mut().unwrap().set("StructTreeRoot", root);
    let doc = open(source);
    let mut r = input::default_request();
    r.outputs = PDF_STRUCTURE;
    let selection = [2];
    r.pages = PdfPageNumbers {
        ptr: selection.as_ptr(),
        len: 1,
    };
    let result = doc.run(&r);
    let nodes = unsafe {
        input::slice(
            result.get().structure_nodes.ptr,
            result.get().structure_nodes.len,
        )
        .unwrap()
    };
    assert_eq!(nodes.len(), 2);
    assert_eq!(
        (nodes[0].id, nodes[0].parent, nodes[1].id, nodes[1].parent),
        (1, 0, 2, 1)
    );
    assert_eq!(unsafe { string(nodes[0].role) }, "Sect");
    assert_eq!(unsafe { string(nodes[1].role) }, "Figure");
    assert_eq!(unsafe { string(nodes[1].alt_text) }, "Revenue chart");
    assert_eq!(unsafe { string(nodes[1].actual_text) }, "Revenue rose");
    assert_eq!(unsafe { string(nodes[1].language) }, "en-US");
    let refs = unsafe { input::slice(nodes[1].references.ptr, nodes[1].references.len).unwrap() };
    assert_eq!((refs[0].page, refs[0].mcid), (2, 7));
    assert_eq!(result.get().present & PDF_ANALYSIS, 0);
    drop(doc);
    assert_eq!(unsafe { string(nodes[1].alt_text) }, "Revenue chart");
}

#[test]
fn runtime_preparation_is_explicit_and_uses_owned_diagnostics() {
    unsafe {
        let mut options = PdfRuntimeOptions::default();
        assert_eq!(pdf_inspector_runtime_options_init(&mut options), PDF_OK);
        assert_eq!(options.download_policy, PDF_DOWNLOAD_OFFLINE);
        let mut result = std::ptr::dangling_mut();
        let mut error = null_mut();
        options.capabilities = PDF_CAP_EXTERNAL_OCR;
        assert_eq!(
            pdf_inspector_prepare_runtime(&options, &mut result, &mut error),
            PDF_INVALID_ARGUMENT
        );
        assert!(result.is_null());
        assert!(!error.is_null());
        pdf_inspector_error_free(error);
        if !cfg!(feature = "ocr") {
            options.capabilities = PDF_CAP_OCR;
            assert_eq!(
                pdf_inspector_prepare_runtime(&options, &mut result, &mut error),
                PDF_UNSUPPORTED
            );
            assert!(result.is_null());
            pdf_inspector_error_free(error);
        }
    }
}

#[cfg(feature = "ocr")]
#[test]
fn offline_runtime_failure_needs_no_document_and_does_not_populate_models() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_str().unwrap();
    let options = PdfRuntimeOptions {
        model_directory: view(path.as_bytes()),
        ..runtime::defaults()
    };
    unsafe {
        let mut result = std::ptr::dangling_mut();
        let mut error = null_mut();
        assert_eq!(
            pdf_inspector_prepare_runtime(&options, &mut result, &mut error),
            PDF_RUNTIME_ERROR
        );
        assert!(result.is_null());
        assert!(!string((*pdf_inspector_error_view(error)).message).is_empty());
        pdf_inspector_error_free(error);
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn external_ocr_fuses_page_space_spans_and_reports_tables() {
    let body = "BT /F1 12 Tf 50 750 Td (Native heading) Tj ET";
    let doc = open(document(&[body, ""], true));
    let mut spans = Vec::new();
    for (row, (left, right)) in [
        ("Metric", "Value"),
        ("Accuracy", "95%"),
        ("Recall", "93%"),
        ("Precision", "96%"),
    ]
    .into_iter()
    .enumerate()
    {
        let y = 260.0 + row as f32 * 20.0;
        spans.push(ocr_span(left, 60.0, y, 70.0));
        spans.push(ocr_span(right, 160.0, y, 70.0));
    }
    let mut ocr = recognition(&spans);
    ocr.page = 2;
    let mut r = input::default_request();
    r.outputs |= PDF_ANALYSIS;
    attach(&mut r, &ocr);
    let result = doc.run(&r);
    let pages = result.pages();
    assert_eq!(pages.len(), 2);
    assert!(unsafe { string(pages[0].markdown) }.contains("Native heading"));
    assert_eq!(pages[0].flags & PDF_PAGE_OCR_RAN, 0);
    let fused = unsafe { string(pages[1].markdown) };
    assert!(fused.contains("Accuracy"), "{fused}");
    assert!(fused.contains("|"), "{fused}");
    assert_ne!(pages[1].flags & PDF_PAGE_HAS_TABLES, 0);
    assert_ne!(pages[1].flags & PDF_PAGE_OCR_RAN, 0);
    assert_eq!(pages[1].provenance.source, PDF_CONTENT_OCR);
    assert_eq!(pages[1].provenance.render_dpi, 0.0);
    let document = unsafe { string(result.get().markdown) };
    assert!(document.contains("Native heading") && document.contains("Accuracy"));
    // Spans on a page that was not selected or is duplicated are rejected.
    ocr.page = 1;
    let both = [ocr, ocr];
    r.external_ocr = PdfOcrPageInputs {
        ptr: both.as_ptr(),
        len: 2,
    };
    unsafe {
        let e = expect_failure(doc.0, &r, PDF_INVALID_ARGUMENT);
        pdf_inspector_error_free(e);
    }
}
