//! Indexed, revisioned selection over the renderer's existing [`PageTextLayout`].
//!
//! Layout ingestion and selection remain independent of browser drawing:
//! ```no_run
//! use std::sync::Arc;
//! use pdf_renderer::{DocumentTextSelection, PageTextLayout, TextSelectionResult};
//! /// Installs a page layout and retrieves its initial selection geometry.
//! fn install(
//!     selection: &mut DocumentTextSelection,
//!     layout: Arc<PageTextLayout>,
//! ) -> TextSelectionResult<()> {
//!     selection.install_layout(0, 1, [800.0, 1000.0], layout)?;
//!     let _updates = selection.updates(&[0], None)?;
//!     Ok(())
//! }
//! ```

use crate::text_selection::{PageTextLayout, TextSelection};
use pdf_graphics::{point::Point, rect::Rect};
use std::sync::Arc;
use thiserror::Error;

/// Errors produced while managing revisioned document text selection.
#[derive(Debug, Error)]
pub enum TextSelectionError {
    /// A page identity, layout dimension, point, or glyph index is invalid.
    #[error("invalid text selection input: {0}")]
    InvalidInput(&'static str),
    /// A revision or glyph index cannot be represented by the selection model.
    #[error("text selection resource limit exceeded")]
    ResourceLimit,
    /// A layout, endpoint, or requested update belongs to an obsolete revision.
    #[error("stale text selection revision")]
    StaleRevision,
}

/// Result returned by revisioned document text-selection operations.
pub type TextSelectionResult<T> = Result<T, TextSelectionError>;

/// A cross-page endpoint adding identity/revision to the existing layout's glyph index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionPoint {
    /// Document page index.
    pub page: u32,
    /// Layout revision assigning this glyph index.
    pub layout_revision: u32,
    /// Inclusive index into PageTextLayout::glyphs, preserving current selection semantics.
    pub glyph_index: usize,
}

/// Direction-preserving custom selection; page ranges use existing TextSelection values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionSpan {
    /// Initial pointer or keyboard anchor.
    pub anchor: SelectionPoint,
    /// Current pointer or keyboard focus.
    pub focus: SelectionPoint,
}

/// Changed-page highlight output in the retained layout's original device coordinates.
pub struct SelectionBatch {
    page: u32,
    layout_revision: u32,
    selection_revision: u32,
    device_size: [f32; 2],
    keys: Vec<u32>,
    bounds: Vec<Rect>,
}

impl SelectionBatch {
    /// Returns the affected document page.
    pub fn page(&self) -> u32 {
        self.page
    }

    /// Returns the layout revision used to produce the geometry.
    pub fn layout_revision(&self) -> u32 {
        self.layout_revision
    }

    /// Returns the selection revision represented by this batch.
    pub fn selection_revision(&self) -> u32 {
        self.selection_revision
    }

    /// Returns the retained layout's logical device dimensions.
    pub fn device_size(&self) -> [f32; 2] {
        self.device_size
    }

    /// Returns stable highlight identifiers in the same order as the bounds.
    pub fn keys(&self) -> &[u32] {
        &self.keys
    }

    /// Returns highlight rectangles in the retained layout's device space.
    pub fn bounds(&self) -> &[Rect] {
        &self.bounds
    }
}

/// A retained page layout and its revision-dependent selection state.
struct IndexedLayout {
    page: u32,
    revision: u32,
    device_size: [f32; 2],
    layout: Arc<PageTextLayout>,
    index: SpatialIndex,
    selection: Option<TextSelection>,
    last_changed_revision: u32,
    available: bool,
}

/// A spatial acceleration structure over valid glyph bounds.
struct SpatialIndex {
    nodes: Vec<IndexNode>,
}

/// An internal spatial-index node containing either a glyph or two children.
struct IndexNode {
    bounds: Rect,
    children: Option<(usize, usize)>,
    glyph: usize,
}

/// Cached custom selection, preserving existing glyph bounds and Unicode copy behavior.
///
/// Layouts/indexes survive display zoom; their device dimensions are retained for
/// mapping to CSS. Selection never invokes PDF rendering or browser font measurement.
/// Index queries return glyph indices; range construction and copied text reuse
/// PageTextLayout rather than defining another extraction/selection implementation.
pub struct DocumentTextSelection {
    pages: Vec<u32>,
    layouts: Vec<IndexedLayout>,
    selected: Option<SelectionSpan>,
    stale: bool,
    revision: u32,
}

impl DocumentTextSelection {
    /// Establishes document order, rejecting duplicate page identities.
    pub fn new(page_order: &[u32]) -> TextSelectionResult<Self> {
        let mut unique = std::collections::BTreeSet::new();
        if page_order.iter().any(|p| !unique.insert(*p)) {
            return Err(TextSelectionError::InvalidInput("duplicate page"));
        }
        Ok(Self {
            pages: page_order.to_vec(),
            layouts: Vec::new(),
            selected: None,
            stale: false,
            revision: 0,
        })
    }

    /// Installs an immutable layout and builds its spatial index once per revision.
    pub fn install_layout(
        &mut self,
        page: u32,
        revision: u32,
        device_size: [f32; 2],
        layout: Arc<PageTextLayout>,
    ) -> TextSelectionResult<()> {
        self.page_position(page)?;
        if device_size.iter().any(|v| !v.is_finite() || *v <= 0.0) {
            return Err(TextSelectionError::InvalidInput("layout dimensions"));
        }
        if let Some(old) = self.layouts.iter().find(|v| v.page == page) {
            if revision < old.revision {
                return Err(TextSelectionError::StaleRevision);
            }
            if revision == old.revision && old.available {
                return if Arc::ptr_eq(&layout, &old.layout) && device_size == old.device_size {
                    Ok(())
                } else {
                    Err(TextSelectionError::StaleRevision)
                };
            }
        }
        let next = self.next_revision()?;
        let index = SpatialIndex::new(&layout);
        self.invalidate_page(page, next);
        self.layouts.retain(|v| v.page != page);
        self.layouts.push(IndexedLayout {
            page,
            revision,
            device_size,
            layout,
            index,
            selection: None,
            last_changed_revision: next,
            available: true,
        });
        self.revision = next;
        Ok(())
    }

    /// Finds a containing or nearest glyph in retained layout coordinates.
    pub fn hit_test(&self, page: u32, point: Point) -> TextSelectionResult<Option<SelectionPoint>> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(TextSelectionError::InvalidInput("selection point"));
        }
        let layout = self.layout(page)?;
        Ok(layout.index.hit(point).map(|glyph_index| SelectionPoint {
            page,
            layout_revision: layout.revision,
            glyph_index,
        }))
    }

    /// Updates selection atomically; missing intermediate layouts and stale endpoints fail.
    pub fn select(&mut self, span: Option<SelectionSpan>) -> TextSelectionResult<u32> {
        if self.selected == span && !self.stale {
            return Ok(self.revision);
        }
        let ranges = if let Some(span) = &span {
            self.ranges(span)?
        } else {
            Vec::new()
        };
        let next = self.next_revision()?;
        for layout in &mut self.layouts {
            let selection = ranges
                .iter()
                .find(|(page, _)| *page == layout.page)
                .and_then(|(_, range)| *range);
            if selection != layout.selection {
                layout.selection = selection;
                layout.last_changed_revision = next;
            }
        }
        self.selected = span;
        self.stale = false;
        self.revision = next;
        Ok(next)
    }

    /// Returns packed geometry only for visible pages changed since the supplied revision.
    pub fn updates(
        &self,
        visible: &[u32],
        since: Option<u32>,
    ) -> TextSelectionResult<Vec<SelectionBatch>> {
        if since.is_some_and(|v| v > self.revision) {
            return Err(TextSelectionError::StaleRevision);
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut output = Vec::new();
        for page in visible {
            self.page_position(*page)?;
            if !seen.insert(*page) {
                continue;
            }
            let Some(layout) = self.layouts.iter().find(|v| v.page == *page) else {
                continue;
            };
            if since.is_some_and(|v| layout.last_changed_revision <= v) {
                continue;
            }
            let mut keys = Vec::new();
            let mut bounds = Vec::new();
            if let Some(selection) = layout.selection {
                for (index, rect) in layout.layout.selection_bounds(selection) {
                    keys.push(u32::try_from(index).map_err(|_| TextSelectionError::ResourceLimit)?);
                    bounds.push(rect);
                }
            }
            output.push(SelectionBatch {
                page: *page,
                layout_revision: layout.revision,
                selection_revision: self.revision,
                device_size: layout.device_size,
                keys,
                bounds,
            });
        }
        Ok(output)
    }

    /// Copies Unicode using PageTextLayout and newlines between document pages.
    pub fn selected_text(&self) -> TextSelectionResult<String> {
        if self.stale {
            return Err(TextSelectionError::StaleRevision);
        }
        let Some(span) = &self.selected else {
            return Ok(String::new());
        };
        let mut text = Vec::new();
        for (page, selection) in self.ranges(span)? {
            text.push(
                selection
                    .map(|s| self.layout(page).map(|v| v.layout.selected_text(s)))
                    .transpose()?
                    .unwrap_or_default(),
            );
        }
        Ok(text.join("\n"))
    }

    /// Releases glyph/index storage and invalidates a dependent selection.
    pub fn evict_page(&mut self, page: u32) -> TextSelectionResult<()> {
        self.page_position(page)?;
        let next = self.next_revision()?;
        self.invalidate_page(page, next);
        if let Some(layout) = self.layouts.iter_mut().find(|v| v.page == page) {
            layout.layout = Arc::new(PageTextLayout::default());
            layout.index = SpatialIndex { nodes: Vec::new() };
            layout.available = false;
            layout.last_changed_revision = next;
        }
        self.revision = next;
        Ok(())
    }

    /// Returns a page's position in document order.
    fn page_position(&self, page: u32) -> TextSelectionResult<usize> {
        self.pages
            .iter()
            .position(|v| *v == page)
            .ok_or(TextSelectionError::InvalidInput("unknown page"))
    }

    /// Returns the available retained layout for a known page.
    fn layout(&self, page: u32) -> TextSelectionResult<&IndexedLayout> {
        self.page_position(page)?;
        self.layouts
            .iter()
            .find(|v| v.page == page && v.available)
            .ok_or(TextSelectionError::StaleRevision)
    }

    /// Returns the next representable selection revision.
    fn next_revision(&self) -> TextSelectionResult<u32> {
        self.revision
            .checked_add(1)
            .ok_or(TextSelectionError::ResourceLimit)
    }

    /// Resolves a direction-preserving document span into normalized page ranges.
    fn ranges(
        &self,
        span: &SelectionSpan,
    ) -> TextSelectionResult<Vec<(u32, Option<TextSelection>)>> {
        for endpoint in [&span.anchor, &span.focus] {
            let layout = self.layout(endpoint.page)?;
            if layout.revision != endpoint.layout_revision {
                return Err(TextSelectionError::StaleRevision);
            }
            if endpoint.glyph_index >= layout.layout.glyphs().len() {
                return Err(TextSelectionError::InvalidInput("glyph index"));
            }
        }
        let a = self.page_position(span.anchor.page)?;
        let b = self.page_position(span.focus.page)?;
        let (start, end) = if (a, span.anchor.glyph_index) <= (b, span.focus.glyph_index) {
            (&span.anchor, &span.focus)
        } else {
            (&span.focus, &span.anchor)
        };
        let mut result = Vec::new();
        for page in self
            .pages
            .iter()
            .skip(a.min(b))
            .take(a.abs_diff(b).saturating_add(1))
        {
            let layout = self.layout(*page)?;
            let first = if *page == start.page {
                start.glyph_index
            } else {
                0
            };
            let last = if *page == end.page {
                end.glyph_index
            } else {
                layout.layout.glyphs().len().saturating_sub(1)
            };
            result.push((*page, layout.layout.selection_from_indices(first, last)));
        }
        Ok(result)
    }

    /// Invalidates selection state that depends on a replaced or evicted page.
    fn invalidate_page(&mut self, page: u32, next: u32) {
        let affected = self.selected.as_ref().is_some_and(|span| {
            let a = self.pages.iter().position(|v| *v == span.anchor.page);
            let b = self.pages.iter().position(|v| *v == span.focus.page);
            let p = self.pages.iter().position(|v| *v == page);
            matches!((a,b,p),(Some(a),Some(b),Some(p)) if (a.min(b)..=a.max(b)).contains(&p))
        });
        if affected {
            self.selected = None;
            self.stale = true;
            for layout in &mut self.layouts {
                if layout.selection.take().is_some() {
                    layout.last_changed_revision = next;
                }
            }
        }
    }
}

impl SpatialIndex {
    /// Builds an index over the layout's valid glyph rectangles.
    fn new(layout: &PageTextLayout) -> Self {
        let mut entries: Vec<_> = layout
            .glyphs()
            .iter()
            .enumerate()
            .filter(|(_, g)| g.bounds.is_valid())
            .map(|(i, g)| (i, g.bounds))
            .collect();
        let mut result = Self { nodes: Vec::new() };
        if !entries.is_empty() {
            result.build(&mut entries);
        }
        result
    }

    /// Recursively builds a balanced binary index and returns the root node.
    fn build(&mut self, entries: &mut [(usize, Rect)]) -> usize {
        let bounds = entries.iter().fold(
            Rect {
                left: f32::INFINITY,
                top: f32::INFINITY,
                right: f32::NEG_INFINITY,
                bottom: f32::NEG_INFINITY,
            },
            |a, (_, b)| Rect {
                left: a.left.min(b.left),
                top: a.top.min(b.top),
                right: a.right.max(b.right),
                bottom: a.bottom.max(b.bottom),
            },
        );
        let index = self.nodes.len();
        self.nodes.push(IndexNode {
            bounds,
            children: None,
            glyph: entries.first().map(|v| v.0).unwrap_or_default(),
        });
        if entries.len() > 1 {
            entries.sort_by(|a, b| {
                if bounds.width() > bounds.height() {
                    a.1.left.total_cmp(&b.1.left)
                } else {
                    a.1.top.total_cmp(&b.1.top)
                }
            });
            let mid = entries.len() / 2;
            let (left, right) = entries.split_at_mut(mid);
            let children = (self.build(left), self.build(right));
            if let Some(node) = self.nodes.get_mut(index) {
                node.children = Some(children);
            }
        }
        index
    }

    /// Returns the containing or nearest indexed glyph for a device-space point.
    fn hit(&self, point: Point) -> Option<usize> {
        let mut stack = vec![0];
        let mut best: Option<(f32, usize)> = None;
        while let Some(index) = stack.pop() {
            let Some(node) = self.nodes.get(index) else {
                continue;
            };
            let distance = node.bounds.distance(point);
            if best.is_some_and(|(d, _)| distance > d) {
                continue;
            }
            if let Some((a, b)) = node.children {
                stack.push(b);
                stack.push(a);
            } else if best.is_none_or(|(d, i)| distance < d || (distance == d && node.glyph < i)) {
                best = Some((distance, node.glyph));
            }
        }
        best.map(|(_, i)| i)
    }
}

#[cfg(test)]
/// Tests revisioned cross-page selection, invalidation, and indexed hit testing.
mod tests {
    use super::*;
    /// Creates a retained layout with equally spaced glyphs on one line.
    fn layout(text: &str) -> Arc<PageTextLayout> {
        Arc::new(PageTextLayout::new(
            text.chars()
                .enumerate()
                .map(|(i, c)| crate::TextGlyph {
                    unicode: pdf_cmap::UnicodeSequence::from_shared(Arc::from(vec![c])),
                    bounds: Rect {
                        left: f32::from(u16::try_from(i).unwrap()) * 12.0,
                        top: 0.0,
                        right: f32::from(u16::try_from(i).unwrap()) * 12.0 + 10.0,
                        bottom: 10.0,
                    },
                })
                .collect(),
        ))
    }

    /// Creates an endpoint for the initial revision of a page layout.
    fn point(page: u32, index: usize) -> SelectionPoint {
        SelectionPoint {
            page,
            layout_revision: 1,
            glyph_index: index,
        }
    }

    #[test]
    /// Checks reverse cross-page copying and incremental highlight updates.
    fn reverse_cross_page_copy_and_dirty_batches() {
        let mut selection = DocumentTextSelection::new(&[7, 3]).unwrap();
        selection
            .install_layout(7, 1, [100.0, 100.0], layout("abc"))
            .unwrap();
        selection
            .install_layout(3, 1, [100.0, 100.0], layout("de"))
            .unwrap();
        let revision = selection
            .select(Some(SelectionSpan {
                anchor: point(3, 1),
                focus: point(7, 1),
            }))
            .unwrap();
        assert_eq!(selection.selected_text().unwrap(), "bc\nde");
        assert_eq!(selection.updates(&[7, 3], None).unwrap().len(), 2);
        assert!(
            selection
                .updates(&[7, 3], Some(revision))
                .unwrap()
                .is_empty()
        );
        selection.select(None).unwrap();
        let cleared = selection.updates(&[7, 3], Some(revision)).unwrap();
        assert_eq!(cleared.len(), 2);
        assert!(cleared.iter().all(|b| b.bounds.is_empty()));
    }

    #[test]
    /// Checks that eviction invalidates copying and clears dependent highlights.
    fn eviction_invalidates_selection_and_clears_retained_pages() {
        let mut selection = DocumentTextSelection::new(&[0, 1]).unwrap();
        for page in [0, 1] {
            selection
                .install_layout(page, 1, [100.0, 100.0], layout("abc"))
                .unwrap();
        }
        let revision = selection
            .select(Some(SelectionSpan {
                anchor: point(0, 0),
                focus: point(1, 2),
            }))
            .unwrap();
        selection.evict_page(1).unwrap();
        assert!(matches!(
            selection.selected_text(),
            Err(TextSelectionError::StaleRevision)
        ));
        assert!(
            selection
                .updates(&[0, 1], Some(revision))
                .unwrap()
                .iter()
                .all(|b| b.bounds.is_empty())
        );
        selection.select(None).unwrap();
        assert_eq!(selection.selected_text().unwrap(), "");
    }

    #[test]
    /// Checks that selection fails when an intermediate page has no layout.
    fn missing_intermediate_layout_is_not_silently_skipped() {
        let mut selection = DocumentTextSelection::new(&[0, 1, 2]).unwrap();
        for page in [0, 2] {
            selection
                .install_layout(page, 1, [100.0, 100.0], layout("a"))
                .unwrap();
        }
        assert!(
            selection
                .select(Some(SelectionSpan {
                    anchor: point(0, 0),
                    focus: point(2, 0)
                }))
                .is_err()
        );
    }

    #[test]
    /// Checks indexed hits against linear page hit testing across nearby points.
    fn indexed_hits_match_upstream_nearest_glyphs() {
        let layout = layout("abcdefghijk");
        let index = SpatialIndex::new(&layout);
        for x in -20_i16..150 {
            for y in [-10.0, 5.0, 20.0] {
                assert_eq!(
                    index.hit(Point::new(f32::from(x), y)),
                    layout.hit_test(f32::from(x), y).map(|h| h.index())
                );
            }
        }
    }

    #[test]
    /// Checks that obsolete revisions and invalid glyph indices are rejected.
    fn rejects_stale_or_out_of_bounds_endpoints() {
        let mut selection = DocumentTextSelection::new(&[0]).unwrap();
        selection
            .install_layout(0, 1, [100.0, 100.0], layout("a"))
            .unwrap();
        assert!(
            selection
                .select(Some(SelectionSpan {
                    anchor: point(0, 0),
                    focus: point(0, 1)
                }))
                .is_err()
        );
        selection
            .install_layout(0, 2, [100.0, 100.0], layout("a"))
            .unwrap();
        assert!(
            selection
                .select(Some(SelectionSpan {
                    anchor: point(0, 0),
                    focus: point(0, 0)
                }))
                .is_err()
        );
    }
}
