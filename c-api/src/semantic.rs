use super::*;
use lopdf::{Document, ObjectId};
use pdf_inspector::structure_tree::{StructElement, StructTree};
use std::collections::{HashMap, HashSet};

pub(super) fn nodes(s: &mut Storage, doc: &Document, selected: &[u32]) -> PdfStructureNodes {
    let mut out = Vec::new();
    if let Some(tree) = StructTree::from_doc(doc) {
        let pages = doc.get_pages();
        let full = selected.len() == pages.len();
        let page_map = pages.into_iter().map(|(page, id)| (id, page)).collect();
        let selected = selected.iter().copied().collect();
        for node in &tree.children {
            visit(s, node, 0, &page_map, &selected, full, &mut out);
        }
    }
    s.structure_nodes(out)
}

#[allow(clippy::too_many_arguments)]
fn visit(
    s: &mut Storage,
    node: &StructElement,
    parent: usize,
    pages: &HashMap<ObjectId, u32>,
    selected: &HashSet<u32>,
    full: bool,
    out: &mut Vec<PdfStructureNode>,
) -> bool {
    let references: Vec<_> = node
        .content_refs
        .iter()
        .filter_map(|r| {
            let page = r
                .page_id
                .and_then(|id| pages.get(&id).copied())
                .unwrap_or(0);
            (page == 0 || selected.contains(&page))
                .then_some(PdfContentReference { page, mcid: r.mcid })
        })
        .collect();
    // Preserve unresolvable semantic leaves rather than guessing their page.
    let mut keep = full
        || !references.is_empty()
        || (node.content_refs.is_empty() && node.children.is_empty());
    let start = out.len();
    let id = start + 1;
    out.push(PdfStructureNode {
        id,
        parent,
        role: s.bytes(node.role.name().as_bytes()),
        alt_text: s.optional(node.alt_text.as_deref()),
        actual_text: s.optional(node.actual_text.as_deref()),
        language: s.optional(node.lang.as_deref()),
        references: s.content_references(references),
    });
    for child in &node.children {
        keep |= visit(s, child, id, pages, selected, full, out);
    }
    if !keep {
        out.truncate(start);
    }
    keep
}
