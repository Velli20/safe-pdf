//! One-time PDF adoption into document-owned Core state.
use crate::{
    commands::{AnnotationCommand, Operation},
    engine::AnnotationEngine,
    events::Change,
    kind::SourceLayout,
    layer_error::{AnnotationLayerError, AnnotationLayerResult},
    models::{AnnotationId, DocumentDescriptor, DocumentId, NewAnnotation, PageIndex},
    pdf_data::{NativeAnnotation as Source, SourceAnnotation},
    session_store::SessionStore,
    source_fields::{SourceFieldGroup, resolve_source_field},
    widgets::WidgetBinding,
};
use pdf_object_reader::{
    diagnostic::{PdfReadDiagnostic, PdfReadDiagnosticKind},
    object_id::ObjectId,
};
use std::collections::BTreeMap;

pub(crate) type Engine = AnnotationEngine<SessionStore>;

/// Widgets sharing a terminal field group together; a widget without one stands alone
/// under its page and source identity.
type SourceFieldKey = (Option<u64>, u32, u64);

/// Source layout retained beside a Core annotation. Presentation text, style, and
/// subtype are read from the annotation's retained source instead.
#[derive(Clone, Debug)]
pub(crate) struct LayoutHint {
    /// Position in the page's source traversal, for stable presentation order.
    pub order: usize,
    /// Normalized copy of the source `/Rect` and its anchor; dropped once the host replaces
    /// the content so stale source geometry never positions an edited payload.
    pub layout: Option<SourceLayout>,
}

impl LayoutHint {
    /// Source layout Core can size and position live bounds from, once the rect is retained.
    pub fn source_layout(&self) -> Option<SourceLayout> {
        self.layout
    }
}

pub(crate) struct ImportedDocument {
    pub engine: Engine,
    pub hints: BTreeMap<u64, LayoutHint>,
    pub diagnostics: Vec<PdfReadDiagnostic>,
}

/// One source annotation at its position in the page's traversal.
#[derive(Clone, Copy)]
struct SourceRecord<'a> {
    page: u32,
    order: usize,
    source: &'a SourceAnnotation,
}

impl SourceRecord<'_> {
    /// Records why this source was not adopted. The source retains only its object
    /// number, so the generation is reported as zero; inline dictionaries have no object.
    fn reject(&self, error: impl std::fmt::Display) -> PdfReadDiagnostic {
        let object = self
            .source
            .object_number
            .and_then(|number| usize::try_from(number).ok())
            .map(|number| ObjectId::new(number, 0));
        PdfReadDiagnostic::new(PdfReadDiagnosticKind::AnnotationImport, None, object, error)
            .on_page(self.page)
    }

    fn hint(&self) -> LayoutHint {
        LayoutHint {
            order: self.order,
            layout: self.source.layout(),
        }
    }
}

/// Adopts one document's sources in three passes: standalone annotations first, then
/// field groups, then popups, which need their parents' Core identities.
struct Importer<'a> {
    engine: Engine,
    hints: BTreeMap<u64, LayoutHint>,
    diagnostics: Vec<PdfReadDiagnostic>,
    /// Core identity of each adopted source by page and object number, for popup parents.
    objects: BTreeMap<(u32, u64), AnnotationId>,
    groups: BTreeMap<SourceFieldKey, Vec<SourceRecord<'a>>>,
    popups: Vec<(SourceRecord<'a>, Option<u64>)>,
}

impl<'a> Importer<'a> {
    fn new(descriptor: DocumentDescriptor) -> AnnotationLayerResult<Self> {
        Ok(Self {
            engine: Engine::new(descriptor, SessionStore)?,
            hints: BTreeMap::new(),
            diagnostics: Vec::new(),
            objects: BTreeMap::new(),
            groups: BTreeMap::new(),
            popups: Vec::new(),
        })
    }

    /// Adopts every standalone source and queues widgets and popups for later passes.
    fn collect(&mut self, pages: &[&'a [SourceAnnotation]]) -> AnnotationLayerResult<()> {
        for (page, sources) in pages.iter().enumerate() {
            let page = u32::try_from(page).map_err(|_| AnnotationLayerError::ResourceLimit)?;
            for (order, source) in sources.iter().enumerate() {
                let record = SourceRecord {
                    page,
                    order,
                    source,
                };
                match &source.kind {
                    Source::Widget(v) => {
                        let key = v
                            .field_id
                            .map_or((None, page, source.id), |id| (Some(id), 0, 0));
                        self.groups.entry(key).or_default().push(record);
                    }
                    Source::Popup(v) => self.popups.push((record, v.parent)),
                    _ => self.adopt(record, None, None),
                }
            }
        }
        Ok(())
    }

    /// Inserts one source, retaining its identity or the reason it was rejected.
    fn adopt(
        &mut self,
        record: SourceRecord<'a>,
        binding: Option<WidgetBinding>,
        parent: Option<AnnotationId>,
    ) {
        match insert(&mut self.engine, record, binding, parent) {
            Ok(id) => self.remember(record, id),
            Err(error) => self.diagnostics.push(record.reject(error)),
        }
    }

    fn remember(&mut self, record: SourceRecord<'a>, id: AnnotationId) {
        if let Some(object) = record.source.object_number {
            self.objects.insert((record.page, object), id);
        }
        self.hints.insert(id.0, record.hint());
    }

    /// Creates each queued field group, or rejects all of its members together.
    fn import_groups(&mut self, descriptor: DocumentDescriptor) -> AnnotationLayerResult<()> {
        // One disposable engine preflights every group: a bad member cannot leave a partial group.
        let mut scratch = Engine::new(descriptor, SessionStore)?;
        let groups = std::mem::take(&mut self.groups);
        for members in groups.values() {
            let sources: Vec<_> = members.iter().map(|record| record.source).collect();
            let preflight = resolve_source_field(&sources)
                .map_err(AnnotationLayerError::from)
                .and_then(|group| insert_group(&mut scratch, members, &group).map(|_| group));
            match preflight {
                Ok(group) => {
                    let ids = insert_group(&mut self.engine, members, &group)?;
                    for (record, id) in members.iter().zip(ids) {
                        self.remember(*record, id);
                    }
                }
                Err(error) => self
                    .diagnostics
                    .extend(members.iter().map(|record| record.reject(&error))),
            }
        }
        Ok(())
    }

    /// Adopts queued popups whose parents were imported on the same page.
    fn import_popups(&mut self) {
        for (record, parent_object) in std::mem::take(&mut self.popups) {
            let parent = match parent_object {
                Some(object) => match self.objects.get(&(record.page, object)) {
                    Some(id) => Some(*id),
                    None => {
                        self.diagnostics
                            .push(record.reject("popup parent was not imported on this page"));
                        continue;
                    }
                },
                None => None,
            };
            self.adopt(record, None, parent);
        }
    }

    fn finish(mut self) -> ImportedDocument {
        self.diagnostics
            .sort_by_key(|diagnostic| (diagnostic.page, diagnostic.object));
        ImportedDocument {
            engine: self.engine,
            hints: self.hints,
            diagnostics: self.diagnostics,
        }
    }
}

/// Adopts one source annotation slice per page, in page order.
pub(crate) fn import<'a>(
    pages: impl IntoIterator<Item = &'a [SourceAnnotation]>,
) -> AnnotationLayerResult<ImportedDocument> {
    let pages = pages.into_iter().collect::<Vec<_>>();
    let descriptor = DocumentDescriptor {
        id: DocumentId("session".into()),
        page_count: u32::try_from(pages.len()).map_err(|_| AnnotationLayerError::ResourceLimit)?,
    };
    let mut importer = Importer::new(descriptor.clone())?;
    importer.collect(&pages)?;
    importer.import_groups(descriptor)?;
    importer.import_popups();
    Ok(importer.finish())
}

fn dispatch(engine: &mut Engine, operation: Operation) -> AnnotationLayerResult<Change> {
    Ok(engine
        .dispatch(AnnotationCommand {
            expected_revision: engine.view().data().revision,
            operation,
        })?
        .change)
}

fn insert(
    engine: &mut Engine,
    record: SourceRecord<'_>,
    binding: Option<WidgetBinding>,
    parent: Option<AnnotationId>,
) -> AnnotationLayerResult<AnnotationId> {
    let annotation =
        NewAnnotation::from_source(PageIndex(record.page), record.source, binding, parent)?;
    match dispatch(engine, Operation::Create { annotation })? {
        Change::Created(id) => Ok(id),
        _ => Err(AnnotationLayerError::UnexpectedChange),
    }
}

/// Creates one shared field and every widget bound to it, in member order.
fn insert_group(
    engine: &mut Engine,
    members: &[SourceRecord<'_>],
    group: &SourceFieldGroup,
) -> AnnotationLayerResult<Vec<AnnotationId>> {
    let field = group.field.clone();
    let field = match dispatch(engine, Operation::CreateField { field })? {
        Change::FieldCreated(id) => id,
        _ => return Err(AnnotationLayerError::UnexpectedChange),
    };
    members
        .iter()
        .zip(group.bindings(field))
        .map(|(record, binding)| insert(engine, *record, Some(binding), None))
        .collect()
}
