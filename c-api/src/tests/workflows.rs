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
fn analysis_needs_no_markdown_or_hints() {
    let doc = Doc::open("real-estate-pricing");
    let mut r = input::default_request();
    r.outputs = PDF_OUT_ANALYSIS | PDF_OUT_ITEMS;
    let result = doc.run(&r);
    assert!(result.get().markdown.ptr.is_null());
    assert!(result.pages().iter().all(|p| p.markdown.ptr.is_null()));
    assert_eq!(result.get().present & PDF_OUT_ANALYSIS, PDF_OUT_ANALYSIS);
    assert!(result
        .pages()
        .iter()
        .any(|p| p.flags & PDF_PAGE_HAS_TABLES != 0));
    assert!(result
        .pages()
        .iter()
        .any(|p| p.flags & PDF_PAGE_HAS_COLUMNS != 0));
    for page in result.pages() {
        let columns = unsafe { input::slice(page.columns.ptr, page.columns.len).unwrap() };
        assert!(columns.iter().all(|column| column.x1 >= column.x0));
        if columns.len() >= 2 {
            assert_ne!(page.flags & PDF_PAGE_HAS_COLUMNS, 0);
            assert_ne!(page.reading_order, PDF_READING_SINGLE);
        }
    }
    // Without table queries nothing is published as a table.
    assert_eq!(result.get().tables.len, 0);
    assert!(result.get().tables.ptr.is_null());
}

#[test]
fn analysis_reports_garbled_text_without_text_output() {
    let doc = Doc::open("shifted_cipher_tounicode");
    let mut r = input::default_request();
    r.outputs = PDF_OUT_ANALYSIS;
    let result = doc.run(&r);
    assert_ne!(result.get().flags & PDF_DOC_ENCODING_ISSUES, 0);
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
    r.outputs = PDF_OUT_STRUCTURE;
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
    assert_eq!((nodes[0].parent, nodes[1].parent), (0, 1));
    assert_eq!(unsafe { string(nodes[0].role) }, "Sect");
    assert_eq!(unsafe { string(nodes[1].role) }, "Figure");
    assert_eq!(unsafe { string(nodes[1].alt_text) }, "Revenue chart");
    assert_eq!(unsafe { string(nodes[1].actual_text) }, "Revenue rose");
    assert_eq!(unsafe { string(nodes[1].language) }, "en-US");
    let refs = unsafe { input::slice(nodes[1].references.ptr, nodes[1].references.len).unwrap() };
    assert_eq!((refs[0].page, refs[0].mcid), (2, 7));
    assert_eq!(result.get().present & PDF_OUT_ANALYSIS, 0);
    drop(doc);
    assert_eq!(unsafe { string(nodes[1].alt_text) }, "Revenue chart");
}

#[test]
fn runtime_preparation_is_explicit_and_uses_owned_diagnostics() {
    unsafe {
        let mut options = PdfRuntimeOptions::default();
        assert_eq!(pdf_inspector_runtime_options_init(&mut options), PDF_OK);
        assert_eq!(options.download_policy, PDF_DOWNLOAD_OFFLINE);
        let mut info = PdfRuntimeInfo {
            capabilities: 0xdead_beef,
            ..PdfRuntimeInfo::default()
        };
        let mut error = null_mut();
        options.capabilities = PDF_CAP_EXTERNAL_OCR;
        assert_eq!(
            pdf_inspector_prepare_runtime(&options, &mut info, &mut error),
            PDF_INVALID_ARGUMENT
        );
        assert_eq!(info.capabilities, 0);
        assert!(!error.is_null());
        pdf_inspector_error_free(error);
        assert_eq!(
            pdf_inspector_prepare_runtime(&options, null_mut(), &mut error),
            PDF_INVALID_ARGUMENT
        );
        pdf_inspector_error_free(error);
        if !cfg!(feature = "ocr") {
            options.capabilities = PDF_CAP_OCR;
            assert_eq!(
                pdf_inspector_prepare_runtime(&options, &mut info, &mut error),
                PDF_UNSUPPORTED
            );
            assert_eq!(info.capabilities, 0);
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
        let mut info = PdfRuntimeInfo::default();
        let mut error = null_mut();
        assert_eq!(
            pdf_inspector_prepare_runtime(&options, &mut info, &mut error),
            PDF_RUNTIME_ERROR
        );
        assert!(info.model.ptr.is_null());
        assert!(!string((*error).message).is_empty());
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
    r.outputs |= PDF_OUT_ANALYSIS;
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

#[test]
fn raster_coverage_measures_drawn_images_without_double_counting_or_resource_inventory() {
    let doc = open(document(
        &[
            "q 600 0 0 800 0 0 cm /Im1 Do Q",
            "",
            "q 300 0 0 800 0 0 cm /Im1 Do /Im1 Do Q",
        ],
        true,
    ));
    let mut request = input::default_request();
    for output in [PDF_OUT_ANALYSIS, PDF_OUT_RASTER] {
        request.outputs = output;
        let result = doc.run(&request);
        let pages = result.pages();
        assert_eq!(pages.len(), 3);
        for page in pages {
            assert_ne!(page.flags & PDF_PAGE_IMAGE_COVERAGE, 0);
        }
        assert_eq!(pages[0].image_coverage, 1.0);
        assert_eq!(pages[1].image_coverage, 0.0);
        assert_eq!(pages[2].image_coverage, 0.5);
        if output == PDF_OUT_RASTER {
            let reasons = unsafe {
                input::slice(pages[0].ocr_reasons.ptr, pages[0].ocr_reasons.len).unwrap()
            }
            .iter()
            .map(|&reason| unsafe { string(reason) })
            .collect::<Vec<_>>();
            assert_eq!(
                reasons,
                ["scanned"],
                "small images still establish an image-only scan"
            );
            assert_eq!(
                pages[1].ocr_reasons.len, 0,
                "unused resources are not a painted scan"
            );
        }
    }
}

#[test]
fn raster_signals_are_page_local_even_outside_the_detection_sample() {
    let paint = "q 600 0 0 800 0 0 cm /Im1 Do Q";
    let masthead = format!("{paint} BT /F1 12 Tf (FINANCIAL EXPRESS 2024) Tj ET");
    let hidden = format!("{paint} BT /F1 12 Tf 3 Tr (Hidden layer) Tj ET");
    let body = format!(
        "{paint} BT /F1 12 Tf {} ET",
        "(Ordinary body text with diverse letters) Tj ".repeat(12)
    );
    let mut source = document(&["", &masthead, &hidden, &body], true);
    // A high-resolution background meets the core's template-image gate.
    for object in source.objects.values_mut() {
        if let Object::Stream(stream) = object {
            if stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image") {
                stream.dict.set("Width", 1000);
                stream.dict.set("Height", 1000);
            }
        }
    }
    let doc = open(source);
    let sample = [1];
    let mut request = input::default_request();
    request.outputs = PDF_OUT_RASTER;
    request.detection.strategy = PDF_SCAN_PAGES;
    request.detection.pages = PdfPageNumbers {
        ptr: sample.as_ptr(),
        len: sample.len(),
    };
    for (number, expected) in [
        (1, vec![]),
        (2, vec!["scanned"]),
        (3, vec!["invisible_text_layer", "scanned"]),
        (4, vec![]),
    ] {
        request.pages = PdfPageNumbers {
            ptr: &number,
            len: 1,
        };
        let result = doc.run(&request);
        assert_eq!(result.get().pages_sampled, 1);
        assert_eq!(result.get().present, PDF_OUT_RASTER | PDF_OUT_INSPECTION);
        let page = &result.pages()[0];
        assert_eq!(page.info.page, number);
        assert_eq!(page.image_coverage, if number == 1 { 0.0 } else { 1.0 });
        let reasons = unsafe { input::slice(page.ocr_reasons.ptr, page.ocr_reasons.len).unwrap() }
            .iter()
            .map(|&reason| unsafe { string(reason) })
            .collect::<Vec<_>>();
        assert_eq!(reasons, expected);
        assert_eq!(page.flags & PDF_PAGE_NEEDS_OCR != 0, !expected.is_empty());
        assert_eq!(
            page.flags & (PDF_PAGE_HAS_TABLES | PDF_PAGE_HAS_COLUMNS | PDF_PAGE_SKIPPED_INVISIBLE),
            0
        );
        assert!(page.markdown.ptr.is_null());
        assert!(page.items.ptr.is_null());
    }
}

#[test]
fn page_roles_match_semantic_structure_and_survive_owner_release() {
    let doc = Doc::open("bare_name_struct");
    let mut request = input::default_request();
    request.outputs = PDF_OUT_STRUCTURE;
    let full = doc.run(&request);
    request.outputs = PDF_OUT_STRUCTURE_ROLES;
    let roles = doc.run(&request);
    let entries = |result: &Owned| {
        result
            .pages()
            .iter()
            .flat_map(|page| unsafe {
                input::slice(page.structure.ptr, page.structure.len).unwrap()
            })
            .map(|entry| (entry.page, entry.mcid, unsafe { string(entry.role) }))
            .collect::<Vec<_>>()
    };
    let expected = entries(&full);
    assert!(!expected.is_empty());
    assert_eq!(entries(&roles), expected);
    assert!(roles.get().structure_nodes.ptr.is_null());
    assert_eq!(
        roles.get().present,
        PDF_OUT_STRUCTURE_ROLES | PDF_OUT_INSPECTION
    );
    drop(full);
    drop(doc);
    assert_eq!(entries(&roles), expected);
}

#[test]
fn raster_coverage_refuses_undecodable_selected_content() {
    let mut source = document(&["", ""], false);
    let page = source.get_pages()[&2];
    let stream = source.get_page_contents(page)[0];
    source
        .get_object_mut(stream)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .dict
        .set("Filter", "UnsupportedDecoder");
    let doc = open(source);
    let mut request = input::default_request();
    request.outputs = PDF_OUT_RASTER;
    let result = doc.run(&request);
    assert_ne!(result.pages()[0].flags & PDF_PAGE_IMAGE_COVERAGE, 0);
    assert_eq!(result.pages()[0].image_coverage, 0.0);
    assert_eq!(result.pages()[1].flags & PDF_PAGE_IMAGE_COVERAGE, 0);
}

#[test]
fn page_units_are_page_local_and_invalid_values_fail_at_open() {
    let mut source = document(&["", ""], false);
    let pages = source.get_pages();
    source
        .get_dictionary_mut(pages[&1])
        .unwrap()
        .set("UserUnit", 1.25f32);
    let parent = source
        .get_dictionary(pages[&1])
        .unwrap()
        .get(b"Parent")
        .unwrap()
        .as_reference()
        .unwrap();
    source
        .get_dictionary_mut(parent)
        .unwrap()
        .set("UserUnit", 9.0f32);
    let doc = open(source.clone());
    let infos = unsafe { input::slice(doc.info().pages.ptr, doc.info().pages.len).unwrap() };
    assert_eq!(infos[0].user_unit, 1.25);
    assert_eq!(infos[1].user_unit, 1.0);
    source
        .get_dictionary_mut(pages[&1])
        .unwrap()
        .set("UserUnit", 75_000);
    let maximum = open(source.clone());
    assert_eq!(
        unsafe { input::slice(maximum.info().pages.ptr, 1).unwrap() }[0].user_unit,
        75_000.0
    );
    for invalid in [0, -1, 75_001] {
        source
            .get_dictionary_mut(pages[&1])
            .unwrap()
            .set("UserUnit", invalid);
        let mut bytes = Vec::new();
        source.save_to(&mut bytes).unwrap();
        unsafe {
            let input = PdfSource {
                kind: PDF_SOURCE_BYTES,
                data: view(&bytes),
                password: PdfBytes::default(),
            };
            let mut handle = null_mut();
            let mut error = null_mut();
            assert_eq!(
                pdf_inspector_open(&input, &mut handle, &mut error),
                PDF_PARSE_ERROR
            );
            assert!(handle.is_null());
            pdf_inspector_error_free(error);
        }
    }
}
