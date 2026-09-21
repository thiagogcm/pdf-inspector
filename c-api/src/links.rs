//! Dest/GoTo annotations as extra C link items. URI links stay in core items.

use super::*;
use lopdf::{Dictionary, Document, Object, ObjectId};
use pdf_inspector::PositionFrame;
use std::collections::{HashMap, HashSet};

pub(super) struct DestLink {
    pub page: u32,
    pub dest_page: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub(super) fn dest_items(
    storage: &Storage,
    doc: &Document,
    selected: &[u32],
    state: &DocumentState,
    frame: PositionFrame,
) -> Vec<PdfItem> {
    extract(doc, selected)
        .into_iter()
        .filter_map(|link| {
            let info = state.frame_info(link.page).ok()?;
            Some(PdfItem {
                page: link.page,
                kind: PDF_ITEM_LINK,
                dest_page: link.dest_page,
                bounds: super::output::user_box_to_view(
                    link.x,
                    link.y,
                    link.width,
                    link.height,
                    &info,
                    frame,
                ),
                text: storage.bytes(""),
                font: storage.bytes(""),
                font_tag: storage.bytes(""),
                link: storage.bytes(""),
                ..PdfItem::default()
            })
        })
        .collect()
}

fn extract(doc: &Document, selected: &[u32]) -> Vec<DestLink> {
    let pages = doc.get_pages();
    let id_to_page: HashMap<ObjectId, u32> = pages.iter().map(|(n, id)| (*id, *n)).collect();
    let selected: HashSet<u32> = selected.iter().copied().collect();
    let mut out = Vec::new();
    for (page, page_id) in &pages {
        if !selected.contains(page) {
            continue;
        }
        let Ok(page_dict) = doc.get_dictionary(*page_id) else {
            continue;
        };
        let Some(annots) = page_dict
            .get(b"Annots")
            .ok()
            .and_then(|a| resolve_array(doc, a))
        else {
            continue;
        };
        for annot in annots {
            let Some(annot_dict) = resolve_dict(doc, annot) else {
                continue;
            };
            if !is_link(annot_dict) || has_uri(doc, annot_dict) {
                continue;
            }
            let Some(dest_page) = dest_page(doc, annot_dict, &id_to_page) else {
                continue;
            };
            let Some((x, y, width, height)) = annot_rect(annot_dict) else {
                continue;
            };
            out.push(DestLink {
                page: *page,
                dest_page,
                x,
                y,
                width,
                height,
            });
        }
    }
    out
}

/// The array behind `obj`, following any indirection.
fn resolve_array<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a [Object]> {
    doc.dereference(obj)
        .ok()?
        .1
        .as_array()
        .ok()
        .map(Vec::as_slice)
}

/// The dictionary behind `obj`, following any indirection.
fn resolve_dict<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Dictionary> {
    doc.dereference(obj).ok()?.1.as_dict().ok()
}

fn is_link(annot: &Dictionary) -> bool {
    annot
        .get(b"Subtype")
        .ok()
        .and_then(|s| s.as_name().ok())
        .is_some_and(|name| name == b"Link")
}

fn has_uri(doc: &Document, annot: &Dictionary) -> bool {
    annot
        .get(b"A")
        .ok()
        .and_then(|action| resolve_dict(doc, action))
        .is_some_and(|d| d.get(b"URI").is_ok())
}

fn annot_rect(annot: &Dictionary) -> Option<(f32, f32, f32, f32)> {
    let arr = annot.get(b"Rect").ok()?.as_array().ok()?;
    if arr.len() < 4 {
        return None;
    }
    let x1 = number(&arr[0])?;
    let y1 = number(&arr[1])?;
    let x2 = number(&arr[2])?;
    let y2 = number(&arr[3])?;
    Some((x1, y1, x2 - x1, y2 - y1))
}

fn dest_page(
    doc: &Document,
    annot: &Dictionary,
    id_to_page: &HashMap<ObjectId, u32>,
) -> Option<u32> {
    if let Ok(dest) = annot.get(b"Dest") {
        return resolve_dest(doc, dest, id_to_page);
    }
    let action = resolve_dict(doc, annot.get(b"A").ok()?)?;
    if action.get(b"S").ok()?.as_name().ok()? != b"GoTo" {
        return None;
    }
    resolve_dest(doc, action.get(b"D").ok()?, id_to_page)
}

fn resolve_dest(doc: &Document, dest: &Object, id_to_page: &HashMap<ObjectId, u32>) -> Option<u32> {
    match doc.dereference(dest).ok()?.1 {
        Object::Array(arr) => dest_array_page(arr, id_to_page),
        Object::Dictionary(dict) => resolve_dest(doc, dict.get(b"D").ok()?, id_to_page),
        Object::Name(name) => named_dest(doc, name, id_to_page),
        Object::String(bytes, _) => named_dest(doc, bytes, id_to_page),
        _ => None,
    }
}

fn dest_array_page(arr: &[Object], id_to_page: &HashMap<ObjectId, u32>) -> Option<u32> {
    match arr.first()? {
        Object::Reference(id) => id_to_page.get(id).copied(),
        Object::Integer(i) if *i >= 0 => {
            let page = *i as u32 + 1;
            id_to_page.values().any(|&p| p == page).then_some(page)
        }
        _ => None,
    }
}

fn named_dest(doc: &Document, name: &[u8], id_to_page: &HashMap<ObjectId, u32>) -> Option<u32> {
    let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let catalog = doc.get_dictionary(root).ok()?;
    if let Ok(dests) = catalog.get(b"Dests") {
        if let Some(page) = dests_dict(doc, dests, name, id_to_page) {
            return Some(page);
        }
    }
    let names_dict = resolve_dict(doc, catalog.get(b"Names").ok()?)?;
    dests_dict(doc, names_dict.get(b"Dests").ok()?, name, id_to_page)
}

fn dests_dict(
    doc: &Document,
    dests: &Object,
    name: &[u8],
    id_to_page: &HashMap<ObjectId, u32>,
) -> Option<u32> {
    let dict = resolve_dict(doc, dests)?;
    if let Ok(value) = dict.get(name) {
        return resolve_dest(doc, value, id_to_page);
    }
    name_tree(doc, dict, name, id_to_page, 0)
}

fn name_tree(
    doc: &Document,
    dict: &Dictionary,
    name: &[u8],
    id_to_page: &HashMap<ObjectId, u32>,
    depth: usize,
) -> Option<u32> {
    if depth > 32 {
        return None;
    }
    let names = dict
        .get(b"Names")
        .ok()
        .and_then(|names| resolve_array(doc, names));
    if let Some(names) = names {
        let mut entries = names.iter();
        while let (Some(key), Some(value)) = (entries.next(), entries.next()) {
            let key = match key {
                Object::Name(n) | Object::String(n, _) => n.as_slice(),
                _ => continue,
            };
            if key == name {
                return resolve_dest(doc, value, id_to_page);
            }
        }
    }
    let kids = resolve_array(doc, dict.get(b"Kids").ok()?)?;
    for kid in kids.iter().take(256) {
        let Object::Reference(id) = kid else {
            continue;
        };
        let Ok(child) = doc.get_dictionary(*id) else {
            continue;
        };
        if let Some(page) = name_tree(doc, child, name, id_to_page, depth + 1) {
            return Some(page);
        }
    }
    None
}

fn number(obj: &Object) -> Option<f32> {
    match obj {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}
