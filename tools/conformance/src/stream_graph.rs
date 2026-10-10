//! The content streams a page can draw through its resources, with cycles marked.
//!
//! Form XObjects, Type 3 glyph procedures, tiling patterns and soft masks are content streams
//! of their own, drawing with resources that can lead back to a stream already being drawn.
//! The graph is read from the resource dictionaries without rendering, so it is available
//! for pages whose rendering crashes or times out.

use pdf_document::page::PdfPage;
use pdf_font::PdfFontSpec;
use pdf_object_reader::ObjectHandle;
use pdf_resources::{
    external_graphics_state::ExternalGraphicsStateKey, pattern::Pattern, resource::Resource,
    resources::Resources,
};
use std::{collections::HashSet, fmt::Write as _, sync::Arc};

/// Lines written at most, so a large resource tree stays readable.
const LINE_LIMIT: usize = 2000;
/// Deepest nesting followed, matching Safe-PDF's limit on active content streams.
const DEPTH_LIMIT: usize = 40;

/// Identity of a node: its indirect object, or the address of a direct value.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum NodeKey {
    Object(usize, usize),
    Direct(usize),
}

impl NodeKey {
    /// Keys a node by its handle's object, or by the address of its value when direct.
    fn of<T, U>(handle: &ObjectHandle<T>, value: &Arc<U>) -> Self {
        handle.object_id().map_or_else(
            || Self::Direct(Arc::as_ptr(value).addr()),
            |id| Self::Object(id.number, id.generation),
        )
    }

    /// Returns ` obj N` for an indirect object, empty for a direct value.
    fn label(self) -> String {
        match self {
            Self::Object(number, 0) => format!(" obj {number}"),
            Self::Object(number, generation) => format!(" obj {number} {generation}"),
            Self::Direct(_) => String::new(),
        }
    }
}

/// A content stream reachable from a resource, with the resources it draws with.
struct Node {
    key: NodeKey,
    /// What the node is, such as `tiling pattern /P1 obj 12`.
    label: String,
    resources: Option<Arc<Resources>>,
}

/// Indented listing of the content streams reachable from a page.
pub struct StreamGraph {
    text: String,
    lines: usize,
    cycles: usize,
    /// Nodes on the path from the page to the node being listed.
    active: Vec<NodeKey>,
    /// Nodes whose subtree was already listed, which later references only name.
    listed: HashSet<NodeKey>,
}

impl StreamGraph {
    /// Lists the content streams reachable from page `index`.
    pub fn of_page(page: &PdfPage, index: usize) -> Self {
        let mut graph = Self {
            text: String::new(),
            lines: 0,
            cycles: 0,
            active: Vec::new(),
            listed: HashSet::new(),
        };
        graph.line(
            0,
            &format!(
                "page {index} content{}",
                page.contents
                    .as_ref()
                    .map(|content| format!(" ({} operators)", content.operators.len()))
                    .unwrap_or_default()
            ),
        );
        if let Some(resources) = &page.resources {
            graph.children(resources, 1);
        }
        graph
    }

    /// Returns the listing with a header explaining its notation.
    pub fn text(&self) -> String {
        let mut text = String::from(
            "# Content streams reachable from the page through its resources\n\
             # Each entry draws with the entries indented beneath it. Type 3 fonts list their\n\
             # glyph procedures. `↻ cycle` marks a stream that is already being drawn on that\n\
             # path, which Safe-PDF does not draw again; `listed above` marks one already expanded.\n",
        );
        let _ = writeln!(text, "# Cycles: {}\n", self.cycles);
        text.push_str(&self.text);
        if self.lines > LINE_LIMIT {
            let _ = writeln!(
                text,
                "… {} more lines",
                self.lines.saturating_sub(LINE_LIMIT)
            );
        }
        text
    }

    fn line(&mut self, depth: usize, label: &str) {
        self.lines = self.lines.saturating_add(1);
        if self.lines <= LINE_LIMIT {
            let _ = writeln!(self.text, "{}- {label}", "  ".repeat(depth));
        }
    }

    /// Lists every content stream drawing with `resources`, in name order per kind.
    fn children(&mut self, resources: &Resources, depth: usize) {
        for node in nodes(resources) {
            match node {
                Ok(node) => self.node(node, depth),
                Err(label) => self.line(depth, &label),
            }
        }
    }

    fn node(&mut self, node: Node, depth: usize) {
        if self.active.contains(&node.key) {
            self.cycles = self.cycles.saturating_add(1);
            self.line(depth, &format!("{} ↻ cycle", node.label));
            return;
        }
        if self.listed.contains(&node.key) {
            self.line(depth, &format!("{} (listed above)", node.label));
            return;
        }
        if depth >= DEPTH_LIMIT {
            self.line(depth, &format!("{} (nesting limit)", node.label));
            return;
        }
        self.line(depth, &node.label);
        self.listed.insert(node.key);
        if let Some(resources) = &node.resources {
            self.active.push(node.key);
            self.children(resources, depth.saturating_add(1));
            self.active.pop();
        }
    }
}

/// Returns the content-stream nodes among `resources`, or a label for an unreadable entry.
fn nodes(resources: &Resources) -> Vec<Result<Node, String>> {
    let mut nodes = Vec::new();
    for (name, handle) in sorted(&resources.fonts) {
        nodes.push(font_node(name, handle).transpose());
    }
    for (name, handle) in sorted(&resources.xobjects) {
        match handle.get() {
            Ok(resource) => {
                if let Resource::Form(form) = resource.as_ref() {
                    nodes.push(Some(form_node(&format!("form /{name}"), form)));
                }
            }
            Err(error) => nodes.push(Some(Err(format!("xobject /{name}: unreadable ({error})")))),
        }
    }
    for (name, resource) in sorted(&resources.patterns) {
        if let Resource::Pattern(handle) = resource {
            nodes.push(pattern_node(name, handle).transpose());
        }
    }
    for (name, resource) in sorted(&resources.ext_g_states) {
        if let Resource::ExternalGraphicsState(handle) = resource {
            nodes.extend(soft_mask_nodes(name, handle).into_iter().map(Some));
        }
    }
    nodes.into_iter().flatten().collect()
}

/// Returns map entries ordered by their printable name.
fn sorted<V>(map: &std::collections::HashMap<Vec<u8>, V>) -> Vec<(String, &V)> {
    let mut entries: Vec<(String, &V)> = map
        .iter()
        .map(|(name, value)| (String::from_utf8_lossy(name).into_owned(), value))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

/// Returns a Type 3 font with its glyph procedures; other fonts draw no content streams.
fn font_node(name: String, handle: &ObjectHandle<Resource>) -> Result<Option<Node>, String> {
    let resource = handle
        .get()
        .map_err(|error| format!("font /{name}: unreadable ({error})"))?;
    let Resource::Font { font, resources } = resource.as_ref() else {
        return Ok(None);
    };
    let PdfFontSpec::Type3(type3) = font.as_ref() else {
        return Ok(None);
    };
    let key = NodeKey::of(handle, font);
    let glyphs: Vec<String> = type3
        .char_procedures
        .keys()
        .map(|glyph| String::from_utf8_lossy(&glyph.0).into_owned())
        .collect();
    let resources = match resources.as_ref().map(ObjectHandle::get).transpose() {
        Ok(resources) => resources,
        Err(error) => {
            return Err(format!(
                "Type3 font /{name}: resources unreadable ({error})"
            ));
        }
    };
    Ok(Some(Node {
        key,
        label: format!(
            "Type3 font /{name}{}, glyphs {}",
            key.label(),
            glyphs.join(", ")
        ),
        resources,
    }))
}

/// Returns a form XObject, also used for the form drawing a soft mask.
fn form_node(
    label: &str,
    handle: &ObjectHandle<pdf_resources::form::FormXObject>,
) -> Result<Node, String> {
    let form = handle
        .get()
        .map_err(|error| format!("{label}: unreadable ({error})"))?;
    let key = NodeKey::of(handle, &form);
    let resources = form
        .resources
        .as_ref()
        .map(ObjectHandle::get)
        .transpose()
        .map_err(|error| format!("{label}{}: resources unreadable ({error})", key.label()))?;
    Ok(Node {
        key,
        label: format!(
            "{label}{} ({} operators)",
            key.label(),
            form.content_stream.operators.len()
        ),
        resources,
    })
}

/// Returns a tiling pattern; shading patterns draw no content stream.
fn pattern_node(name: String, handle: &ObjectHandle<Pattern>) -> Result<Option<Node>, String> {
    let pattern = handle
        .get()
        .map_err(|error| format!("pattern /{name}: unreadable ({error})"))?;
    let Pattern::Tiling {
        resources,
        content_stream,
        ..
    } = pattern.as_ref()
    else {
        return Ok(None);
    };
    let key = NodeKey::of(handle, &pattern);
    let label = format!(
        "tiling pattern /{name}{} ({} operators)",
        key.label(),
        content_stream.operators.len()
    );
    let resources = resources
        .get()
        .map_err(|error| format!("{label}: resources unreadable ({error})"))?;
    Ok(Some(Node {
        key,
        label,
        resources: Some(resources),
    }))
}

/// Returns the forms drawing the soft masks an external graphics state sets.
fn soft_mask_nodes(
    name: String,
    handle: &ObjectHandle<pdf_resources::external_graphics_state::ExternalGraphicsState>,
) -> Vec<Result<Node, String>> {
    let state = match handle.get() {
        Ok(state) => state,
        Err(error) => return vec![Err(format!("graphics state /{name}: unreadable ({error})"))],
    };
    state
        .params
        .iter()
        .filter_map(|param| match param {
            ExternalGraphicsStateKey::SoftMask(Some(mask)) => Some(mask),
            _ => None,
        })
        .map(|mask| {
            let mask = mask
                .get()
                .map_err(|error| format!("soft mask of /{name}: unreadable ({error})"))?;
            form_node(&format!("soft mask of /{name}, form"), &mask.shape)
        })
        .collect()
}
