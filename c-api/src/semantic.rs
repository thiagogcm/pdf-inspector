use super::output::narrow;
use super::*;
use lopdf::{Document, ObjectId};
use pdf_inspector::structure_tree::{StructElement, StructTree};
use std::collections::{HashMap, HashSet};

pub(super) fn nodes(s: &Storage, doc: &Document, selected: &[u32]) -> PdfStructureNodes {
    let mut out = Vec::new();
    if let Some(tree) = StructTree::from_doc(doc) {
        let pages = doc.get_pages();
        let scope = Scope {
            pages: pages.into_iter().map(|(page, id)| (id, page)).collect(),
            selected: selected.iter().copied().collect(),
            full: selected.len() == doc.get_pages().len(),
        };
        let mut kept = HashSet::new();
        for node in &tree.children {
            scope.mark(node, &mut kept);
        }
        for node in &tree.children {
            scope.emit(s, node, 0, &kept, &mut out);
        }
    }
    s.slice(out)
}

struct Scope {
    pages: HashMap<ObjectId, u32>,
    selected: HashSet<u32>,
    full: bool,
}
impl Scope {
    fn references<'a>(
        &'a self,
        node: &'a StructElement,
    ) -> impl Iterator<Item = PdfContentReference> + 'a {
        node.content_refs.iter().filter_map(|r| {
            let page = r
                .page_id
                .and_then(|id| self.pages.get(&id).copied())
                .unwrap_or(0);
            (page == 0 || self.selected.contains(&page))
                .then_some(PdfContentReference { page, mcid: r.mcid })
        })
    }
    /// A node survives when it or a descendant references a selected page.
    /// Unresolvable semantic leaves are preserved rather than guessed at.
    fn mark(&self, node: &StructElement, kept: &mut HashSet<*const StructElement>) -> bool {
        let mut keep = self.full
            || self.references(node).next().is_some()
            || (node.content_refs.is_empty() && node.children.is_empty());
        for child in &node.children {
            keep |= self.mark(child, kept);
        }
        if keep {
            kept.insert(node);
        }
        keep
    }
    fn emit(
        &self,
        s: &Storage,
        node: &StructElement,
        parent: u32,
        kept: &HashSet<*const StructElement>,
        out: &mut Vec<PdfStructureNode>,
    ) {
        if !kept.contains(&(node as *const _)) {
            return;
        }
        let id = narrow(out.len() + 1);
        out.push(PdfStructureNode {
            parent,
            role: s.bytes(node.role.name()),
            alt_text: s.optional(node.alt_text.as_deref()),
            actual_text: s.optional(node.actual_text.as_deref()),
            language: s.optional(node.lang.as_deref()),
            references: s.slice(self.references(node)),
        });
        for child in &node.children {
            self.emit(s, child, id, kept, out);
        }
    }
}
