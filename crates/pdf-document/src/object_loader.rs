//! Dependency-aware loading of objects referenced by a PDF cross-reference table.
//!
//! Loading is not always a single pass: a stream may use an indirect `/Length`,
//! and that length may itself live in a compressed object stream. The loader
//! therefore separates the process into three responsibilities:
//!
//! 1. [`LoadPlan`] partitions xref entries without parsing any objects.
//! 2. [`ObjectLoadQueue`] schedules work and wakes objects whose dependencies load.
//! 3. [`ObjectLoader`] parses, decrypts, and inserts the scheduled objects.
//!
//! This avoids rescanning the entire xref table after every successful load. A
//! deferred object is retried only when the exact object number it needs becomes
//! available.

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::diagnostic::{PdfReadDiagnostic, PdfReadDiagnosticKind};
use crate::error::PdfReaderError;
use crate::object_stream::read_object_stream;
use crate::reader::{EncryptionContext, object_id};
use pdf_object_collection::object_collection::ObjectCollection;
use pdf_object_reader::{
    cross_reference_table::CrossReferenceEntryType, object_error::ObjectError,
};
use pdf_parser::{error::ParserError, parser::PdfParser};

/// Maps an index within one object stream to the xref object numbers using that index.
///
/// Multiple object numbers can target the same index in malformed PDFs. Keeping
/// all of them preserves best-effort behavior while allowing the normal,
/// one-to-one case to move the parsed value without cloning it.
type CompressedStreamPlan = BTreeMap<usize, Vec<usize>>;

/// Work derived from the cross-reference table before object parsing begins.
struct LoadPlan {
    /// Byte offsets of ordinary indirect objects, in initial parsing order.
    normal_objects: VecDeque<usize>,
    /// Compressed-object requests grouped by their containing object stream.
    compressed_streams: BTreeMap<usize, CompressedStreamPlan>,
    /// Number of live xref entries used to preallocate the object collection.
    object_capacity: usize,
}

impl LoadPlan {
    /// Partitions live xref entries into normal objects and compressed-stream batches.
    fn from_entries(entries: &BTreeMap<usize, CrossReferenceEntryType>) -> Self {
        let mut normal_objects = entries
            .values()
            .filter_map(|entry| match entry {
                CrossReferenceEntryType::Normal { byte_offset, .. } if *byte_offset != 0 => {
                    Some(*byte_offset)
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        // Higher-numbered objects commonly contain lengths or metadata needed by
        // earlier objects, so retain the reader's established reverse-xref order.
        normal_objects.reverse();

        // BTreeMap keeps stream and index processing deterministic, including
        // which unresolved stream is reported first.
        let mut compressed_streams = BTreeMap::<usize, CompressedStreamPlan>::new();
        for (&object_number, entry) in entries {
            let CrossReferenceEntryType::Compressed {
                object_stream_number,
                index_within_stream,
            } = entry
            else {
                continue;
            };
            compressed_streams
                .entry(*object_stream_number)
                .or_default()
                .entry(*index_within_stream)
                .or_default()
                .push(object_number);
        }

        let compressed_count = compressed_streams
            .values()
            .flat_map(BTreeMap::values)
            .map(Vec::len)
            .fold(0usize, usize::saturating_add);
        let object_capacity = normal_objects.len().saturating_add(compressed_count);

        Self {
            normal_objects: normal_objects.into(),
            compressed_streams,
            object_capacity,
        }
    }
}

/// Normal objects parked until a referenced object number becomes available.
#[derive(Default)]
struct DeferredObjects {
    /// Maps a missing object number to the offsets that failed while resolving it.
    offsets_by_dependency: HashMap<usize, Vec<usize>>,
}

impl DeferredObjects {
    /// Parks `byte_offset` until `object_number` is inserted.
    fn wait_for(&mut self, object_number: usize, byte_offset: usize) {
        self.offsets_by_dependency
            .entry(object_number)
            .or_default()
            .push(byte_offset);
    }

    /// Removes and returns every offset unblocked by `object_number`.
    fn resume(&mut self, object_number: usize) -> Option<Vec<usize>> {
        self.offsets_by_dependency.remove(&object_number)
    }

    /// Builds the deterministic terminal error for dependencies that never loaded.
    fn unresolved_error(&self) -> Option<PdfReaderError> {
        let first_offset = self
            .offsets_by_dependency
            .values()
            .flatten()
            .copied()
            .min()?;
        let count = self
            .offsets_by_dependency
            .values()
            .map(Vec::len)
            .fold(0usize, usize::saturating_add);
        Some(PdfReaderError::UnresolvedObjects {
            count,
            first_offset,
        })
    }
}

/// A unit of work whose prerequisites are currently available.
enum ObjectLoadTask {
    /// Parses an ordinary indirect object at an xref byte offset.
    Normal {
        /// Absolute byte offset within the PDF input.
        byte_offset: usize,
    },
    /// Parses one object stream and materializes its requested members.
    CompressedStream {
        /// Indirect object number of the containing `/ObjStm`.
        object_stream_number: usize,
        /// Xref members requested from the stream, grouped by stream index.
        plan: CompressedStreamPlan,
    },
}

/// Schedules object work and wakes tasks when their dependencies become available.
#[derive(Default)]
struct ObjectLoadQueue {
    /// Normal-object offsets ready to parse.
    normal_objects: VecDeque<usize>,
    /// Newly inserted object numbers whose dependents have not been woken yet.
    available_objects: VecDeque<usize>,
    /// Object streams whose containing stream object has been inserted.
    ready_streams: VecDeque<(usize, CompressedStreamPlan)>,
    /// Normal objects blocked on an unavailable indirect object.
    deferred_objects: DeferredObjects,
    /// Compressed-stream plans waiting for their containing stream object.
    pending_streams: BTreeMap<usize, CompressedStreamPlan>,
}

impl ObjectLoadQueue {
    /// Seeds the scheduler from the xref-derived load plan.
    fn from_plan(plan: LoadPlan) -> Self {
        Self {
            normal_objects: plan.normal_objects,
            pending_streams: plan.compressed_streams,
            ..Self::default()
        }
    }

    /// Returns the next parse task, applying dependency notifications along the way.
    ///
    /// Normal objects take priority so an object stream is not parsed before all
    /// immediately available indirect values in its dictionary have been loaded.
    fn next_task(&mut self) -> Option<ObjectLoadTask> {
        loop {
            if let Some(byte_offset) = self.normal_objects.pop_front() {
                return Some(ObjectLoadTask::Normal { byte_offset });
            }
            if let Some(object_number) = self.available_objects.pop_front() {
                self.wake_dependents(object_number);
                continue;
            }
            if let Some((object_stream_number, plan)) = self.ready_streams.pop_front() {
                return Some(ObjectLoadTask::CompressedStream {
                    object_stream_number,
                    plan,
                });
            }
            return None;
        }
    }

    /// Parks a normal object until its missing dependency is inserted.
    fn defer(&mut self, byte_offset: usize, dependency: usize) {
        self.deferred_objects.wait_for(dependency, byte_offset);
    }

    /// Announces one inserted object so dependent work can be scheduled.
    fn mark_available(&mut self, object_number: usize) {
        self.available_objects.push_back(object_number);
    }

    /// Announces a batch of objects extracted from the same object stream.
    fn mark_all_available(&mut self, object_numbers: impl IntoIterator<Item = usize>) {
        self.available_objects.extend(object_numbers);
    }

    /// Moves work unblocked by `object_number` into the appropriate ready queue.
    fn wake_dependents(&mut self, object_number: usize) {
        if let Some(offsets) = self.deferred_objects.resume(object_number) {
            self.normal_objects.extend(offsets);
        }
        if let Some(plan) = self.pending_streams.remove(&object_number) {
            self.ready_streams.push_back((object_number, plan));
        }
    }

    /// Reports work that could not be scheduled after all ready queues were drained.
    ///
    /// Unresolved normal objects take precedence over missing object streams,
    /// matching the loader's normal-before-compressed processing order.
    fn terminal_error(&self) -> Option<PdfReaderError> {
        self.deferred_objects.unresolved_error().or_else(|| {
            self.pending_streams
                .first_key_value()
                .map(|(&object_stream_number, _)| {
                    ObjectError::FailedResolveObjectReference {
                        obj_num: object_stream_number,
                    }
                    .into()
                })
        })
    }
}

/// Parses, decrypts, and stores every live object described by an xref table.
///
/// The loader owns parsing policy and object state, while [`ObjectLoadQueue`]
/// owns scheduling state. Keeping those responsibilities separate makes
/// [`Self::load`] a small orchestration loop and keeps dependency mechanics out
/// of parsing code.
pub(super) struct ObjectLoader<'input, 'loader> {
    /// Random-access parser over the source PDF bytes.
    parser: &'loader PdfParser<'input>,
    /// Optional document decryptor and encryption-dictionary exclusion.
    encryption: EncryptionContext,
    /// Objects available for resolving references during subsequent parses.
    objects: ObjectCollection,
    /// Dependency-aware work scheduler.
    queue: ObjectLoadQueue,
    /// Recoverable failures collected for the final read report.
    diagnostics: &'loader mut Vec<PdfReadDiagnostic>,
}

impl<'input, 'loader> ObjectLoader<'input, 'loader> {
    /// Creates an object loader for one parsed cross-reference table.
    pub(super) fn new(
        entries: &BTreeMap<usize, CrossReferenceEntryType>,
        parser: &'loader PdfParser<'input>,
        encryption: EncryptionContext,
        diagnostics: &'loader mut Vec<PdfReadDiagnostic>,
    ) -> Self {
        let plan = LoadPlan::from_entries(entries);
        Self {
            parser,
            encryption,
            objects: ObjectCollection::with_capacity(plan.object_capacity),
            queue: ObjectLoadQueue::from_plan(plan),
            diagnostics,
        }
    }

    /// Drains dependency-ready work and returns the completed object collection.
    ///
    /// Each task either inserts objects, defers a normal object on a precise
    /// dependency, records a recoverable diagnostic, or returns a fatal error.
    /// Once no task remains, unresolved dependencies become a terminal error.
    pub(super) fn load(mut self) -> Result<ObjectCollection, PdfReaderError> {
        let mut queue = std::mem::take(&mut self.queue);
        while let Some(task) = queue.next_task() {
            self.execute_task(task, &mut queue)?;
        }

        if let Some(error) = queue.terminal_error() {
            return Err(error);
        }
        Ok(self.objects)
    }

    /// Dispatches one scheduler task to the corresponding parsing operation.
    fn execute_task(
        &mut self,
        task: ObjectLoadTask,
        queue: &mut ObjectLoadQueue,
    ) -> Result<(), PdfReaderError> {
        match task {
            ObjectLoadTask::Normal { byte_offset } => self.load_normal_object(byte_offset, queue),
            ObjectLoadTask::CompressedStream {
                object_stream_number,
                plan,
            } => {
                let loaded = self.load_compressed_stream(object_stream_number, plan)?;
                queue.mark_all_available(loaded);
                Ok(())
            }
        }
    }

    /// Loads a normal object or records why it cannot be loaded yet.
    fn load_normal_object(
        &mut self,
        byte_offset: usize,
        queue: &mut ObjectLoadQueue,
    ) -> Result<(), PdfReaderError> {
        match self.load_object(byte_offset) {
            Ok(Some(object_number)) => {
                queue.mark_available(object_number);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(error) => self.handle_normal_object_error(byte_offset, error, queue),
        }
    }

    /// Applies retry, diagnostic, and fatal-error policy to a normal-object failure.
    fn handle_normal_object_error(
        &mut self,
        byte_offset: usize,
        error: PdfReaderError,
        queue: &mut ObjectLoadQueue,
    ) -> Result<(), PdfReaderError> {
        if let Some(dependency) = error.unresolved_object_number() {
            queue.defer(byte_offset, dependency);
            return Ok(());
        }
        if !error.is_recoverable_optional_object_error() {
            return Err(error);
        }

        self.diagnostics.push(PdfReadDiagnostic::new(
            PdfReadDiagnosticKind::ObjectParse,
            Some(byte_offset),
            None,
            error,
        ));
        Ok(())
    }

    /// Parses, decrypts, and inserts one normal indirect object.
    ///
    /// The returned object number is used to wake dependent work. `None` means
    /// that parsing was intentionally skipped after a recoverable decryption
    /// failure or that the parsed value did not insert an addressable object.
    fn load_object(&mut self, byte_offset: usize) -> Result<Option<usize>, PdfReaderError> {
        let mut parser = self
            .parser
            .at_offset(byte_offset)
            .map_err(PdfReaderError::ParserError)?;
        let identifier = parser.parse_indirect_object_id().ok_or(
            ParserError::ExpectedIndirectObjectDeclaration {
                position: byte_offset,
            },
        )?;
        let object = parser
            .parse_indirect_object_value_recovering_streams(identifier, &self.objects)
            .map_err(PdfReaderError::ParserError)?;

        let object = match self.encryption.decryptor_for(Some(identifier)) {
            Some(decryptor) => match decryptor.decrypt_object(identifier, object) {
                Ok(object) => object,
                Err(error) => {
                    self.diagnostics.push(PdfReadDiagnostic::new(
                        PdfReadDiagnosticKind::ObjectDecryption,
                        Some(byte_offset),
                        None,
                        error,
                    ));
                    return Ok(None);
                }
            },
            None => object,
        };
        self.objects
            .insert(identifier, object)
            .map_err(PdfReaderError::ObjectError)?;

        Ok(self
            .objects
            .get(identifier.number)
            .is_some()
            .then_some(identifier.number))
    }

    /// Parses one available object stream and inserts all xref-requested members.
    ///
    /// Parsed values are wrapped in `Option` so the normal case can take ownership
    /// by index without cloning. A clone is required only when a malformed xref
    /// maps more than one object number to the same object-stream index.
    ///
    /// Returns the inserted object numbers so the scheduler can wake their dependents.
    fn load_compressed_stream(
        &mut self,
        object_stream_number: usize,
        stream_plan: CompressedStreamPlan,
    ) -> Result<Vec<usize>, PdfReaderError> {
        let stream_object = self.objects.get(object_stream_number).ok_or(
            ObjectError::FailedResolveObjectReference {
                obj_num: object_stream_number,
            },
        )?;
        let stream = stream_object.try_stream(&self.objects)?;
        let mut objects = read_object_stream(stream, &self.objects)?
            .into_iter()
            .map(Some)
            .collect::<Vec<_>>();
        let loaded_capacity = stream_plan
            .values()
            .map(Vec::len)
            .fold(0usize, usize::saturating_add);
        let mut loaded = Vec::with_capacity(loaded_capacity);

        for (index_within_stream, object_numbers) in stream_plan {
            let Some(object) = objects.get_mut(index_within_stream).and_then(Option::take) else {
                continue;
            };
            let Some((&last_object_number, duplicate_object_numbers)) = object_numbers.split_last()
            else {
                continue;
            };

            // Preserve duplicate-index recovery, but move the value for the final
            // target so valid PDFs take the allocation-free path.
            for &object_number in duplicate_object_numbers {
                self.report_compressed_number_mismatch(object_number, object.number);
                self.objects
                    .insert_compressed(object_number, object.value.clone());
                loaded.push(object_number);
            }

            self.report_compressed_number_mismatch(last_object_number, object.number);
            self.objects
                .insert_compressed(last_object_number, object.value);
            loaded.push(last_object_number);
        }

        Ok(loaded)
    }

    /// Records a recoverable diagnostic when xref and object-stream numbers disagree.
    fn report_compressed_number_mismatch(
        &mut self,
        expected_object_number: usize,
        actual_object_number: usize,
    ) {
        if actual_object_number == expected_object_number {
            return;
        }
        self.diagnostics.push(PdfReadDiagnostic::new(
            PdfReadDiagnosticKind::CompressedObject,
            None,
            Some(object_id(expected_object_number)),
            "xref compressed object number differed from its object stream entry",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_plan_partitions_entries_and_groups_compressed_indexes() {
        let entries = BTreeMap::from([
            (0, CrossReferenceEntryType::new_free(0, 65_535)),
            (1, CrossReferenceEntryType::new_normal(10, 0)),
            (2, CrossReferenceEntryType::new_compressed(5, 1)),
            (3, CrossReferenceEntryType::new_compressed(5, 1)),
            (4, CrossReferenceEntryType::new_normal(0, 0)),
            (5, CrossReferenceEntryType::new_normal(50, 0)),
        ]);

        let plan = LoadPlan::from_entries(&entries);

        assert_eq!(plan.normal_objects, VecDeque::from([50, 10]));
        assert_eq!(plan.object_capacity, 4);
        assert_eq!(
            plan.compressed_streams
                .get(&5)
                .and_then(|stream| stream.get(&1)),
            Some(&vec![2, 3])
        );
    }

    #[test]
    fn unresolved_error_is_deterministic_across_dependencies() {
        let mut deferred = DeferredObjects::default();
        deferred.wait_for(9, 80);
        deferred.wait_for(9, 20);
        deferred.wait_for(4, 50);

        let error = deferred.unresolved_error();

        assert!(matches!(
            error,
            Some(PdfReaderError::UnresolvedObjects {
                count: 3,
                first_offset: 20
            })
        ));
    }
}
