//! Physical-order loading for PDFs whose cross-reference metadata is absent.

use std::collections::{BTreeMap, HashMap, HashSet};

use pdf_object_reader::{
    object_error::ObjectError, object_id::ObjectId, object_resolver::ObjectResolver,
    object_variant::ObjectVariant, trailer::Trailer,
};
use pdf_object_collection::object_collection::ObjectCollection;
use pdf_parser::parser::PdfParser;

use crate::{
    diagnostic::{PdfReadDiagnostic, PdfReadDiagnosticKind},
    error::PdfReaderError,
    object_stream::read_object_stream,
    reader::EncryptionContext,
};

/// A parsed top-level object and its physical position in the source PDF.
struct LinearObjectRecord {
    identifier: ObjectId,
    value: ObjectVariant,
    byte_offset: usize,
}

/// Latest physical declarations accumulated during the linear pass.
#[derive(Default)]
struct ParsedLinearObjects {
    records: BTreeMap<usize, LinearObjectRecord>,
}

impl ParsedLinearObjects {
    /// Inserts an object, replacing an earlier physical declaration with the same number.
    fn insert(&mut self, record: LinearObjectRecord) {
        let _ = self.records.insert(record.identifier.number, record);
    }

    /// Returns the records in physical order for deterministic materialization.
    fn into_physical_order(self) -> Vec<LinearObjectRecord> {
        let mut records = self.records.into_values().collect::<Vec<_>>();
        records.sort_by_key(|record| record.byte_offset);
        records
    }
}

impl ObjectResolver for ParsedLinearObjects {
    fn resolve_object<'a>(
        &'a self,
        object: &'a ObjectVariant,
    ) -> Result<&'a ObjectVariant, ObjectError> {
        let mut current = object;
        let mut in_progress = HashSet::new();

        loop {
            let ObjectVariant::Reference(object_number) = current else {
                return Ok(current);
            };
            if !in_progress.insert(*object_number) {
                return Err(ObjectError::CyclicDependency {
                    obj_num: object_number.number,
                });
            }
            current = self
                .records
                .get(&object_number.number)
                .map(|record| &record.value)
                .ok_or(ObjectError::FailedResolveObjectReference {
                    obj_num: object_number.number,
                })?;
        }
    }
}

/// Resolves known objects while treating forward references as absent.
///
/// Object parsing retains references without resolving them. The only normal parser
/// operation that resolves during an indirect-object read is stream `/Length`; mapping
/// an unavailable forward length to PDF `null` selects the parser's existing
/// `endstream` recovery path.
struct ForwardReferenceResolver<'objects> {
    objects: &'objects ParsedLinearObjects,
    null: ObjectVariant,
}

impl<'objects> ForwardReferenceResolver<'objects> {
    /// Wraps the objects already available at the current physical position.
    fn new(objects: &'objects ParsedLinearObjects) -> Self {
        Self {
            objects,
            null: ObjectVariant::Null,
        }
    }
}

impl ObjectResolver for ForwardReferenceResolver<'_> {
    fn resolve_object<'a>(
        &'a self,
        object: &'a ObjectVariant,
    ) -> Result<&'a ObjectVariant, ObjectError> {
        match self.objects.resolve_object(object) {
            Err(ObjectError::FailedResolveObjectReference { .. }) => Ok(&self.null),
            result => result,
        }
    }
}

/// Parsed document state returned by the missing-xref loader.
pub(super) struct LinearLoadResult {
    /// Newest rooted trailer found during the physical pass.
    pub(super) trailer: Trailer,
    /// Decrypted ordinary and compressed objects.
    pub(super) objects: ObjectCollection,
}

/// Parses every top-level indirect object once in physical file order.
pub(super) struct LinearObjectLoader<'input, 'loader> {
    input: &'input [u8],
    password: &'loader [u8],
    diagnostics: &'loader mut Vec<PdfReadDiagnostic>,
    parsed: ParsedLinearObjects,
    trailer: Option<Trailer>,
}

impl<'input, 'loader> LinearObjectLoader<'input, 'loader> {
    /// Creates a direct loader for a PDF with no usable `startxref` marker.
    pub(super) fn new(
        input: &'input [u8],
        password: &'loader [u8],
        diagnostics: &'loader mut Vec<PdfReadDiagnostic>,
    ) -> Self {
        Self {
            input,
            password,
            diagnostics,
            parsed: ParsedLinearObjects::default(),
            trailer: None,
        }
    }

    /// Reads the full physical file, then decrypts and expands the retained objects.
    pub(super) fn load(mut self) -> Result<LinearLoadResult, PdfReaderError> {
        self.parse_physical_objects()?;
        let mut trailer = self.trailer.take().ok_or(PdfReaderError::MissingTrailer)?;
        let encryption = EncryptionContext::from_parsed_objects(
            &mut trailer,
            &self.parsed,
            self.password,
            self.diagnostics,
        )?;
        let (mut objects, mut source_offsets) = self.materialize_objects(encryption)?;
        self.expand_object_streams(&mut objects, &mut source_offsets)?;

        Ok(LinearLoadResult { trailer, objects })
    }

    /// Advances one parser cursor through complete objects, trailers, and file markers.
    fn parse_physical_objects(&mut self) -> Result<(), PdfReaderError> {
        let mut parser = PdfParser::from(self.input);

        while parser.peek_byte().is_some() {
            parser.skip_whitespace_and_comments();
            if parser.peek_byte().is_none() {
                break;
            }
            if parser.skip_eof_marker_as_comment() {
                continue;
            }

            let position = parser.position();
            if parser.peek_byte() == Some(b't') {
                let mut probe = parser.at_offset(position)?;
                if let Ok(trailer) = probe.parse_trailer(&self.parsed) {
                    if trailer.dictionary.get(b"Root").is_some() {
                        self.trailer = Some(trailer);
                    }
                    parser = probe;
                    continue;
                }
            }

            let mut probe = parser.at_offset(position)?;
            let Some(identifier) = probe.parse_indirect_object_id() else {
                let _ = parser.read_byte();
                continue;
            };
            let resolver = ForwardReferenceResolver::new(&self.parsed);
            let value = probe.parse_indirect_object_value(identifier, &resolver)?;
            self.parsed.insert(LinearObjectRecord {
                identifier,
                value,
                byte_offset: position,
            });
            parser = probe;
        }

        Ok(())
    }

    /// Decrypts retained objects and moves them into the document resolver.
    fn materialize_objects(
        &mut self,
        encryption: EncryptionContext,
    ) -> Result<(ObjectCollection, HashMap<usize, usize>), PdfReaderError> {
        let parsed = std::mem::take(&mut self.parsed);
        let records = parsed.into_physical_order();
        let mut objects = ObjectCollection::with_capacity(records.len());
        let mut source_offsets = HashMap::with_capacity(records.len());

        for record in records {
            let value = match encryption.decryptor_for(Some(record.identifier)) {
                Some(decryptor) => {
                    match decryptor.decrypt_object(record.identifier, record.value) {
                        Ok(value) => value,
                        Err(error) => {
                            self.diagnostics.push(PdfReadDiagnostic::new(
                                PdfReadDiagnosticKind::ObjectDecryption,
                                Some(record.byte_offset),
                                Some(record.identifier),
                                error,
                            ));
                            continue;
                        }
                    }
                }
                None => record.value,
            };
            objects.insert(record.identifier, value)?;
            let _ = source_offsets.insert(record.identifier.number, record.byte_offset);
        }

        Ok((objects, source_offsets))
    }

    /// Expands every physical `/ObjStm` and applies source-order precedence.
    fn expand_object_streams(
        &mut self,
        objects: &mut ObjectCollection,
        source_offsets: &mut HashMap<usize, usize>,
    ) -> Result<(), PdfReaderError> {
        let mut pending = object_streams_in_physical_order(objects, source_offsets);

        while !pending.is_empty() {
            let mut deferred = Vec::new();
            let mut first_error = None;
            let mut made_progress = false;

            for (source_offset, object_number) in pending {
                let result = objects
                    .get(&object_number.number)
                    .ok_or(ObjectError::FailedResolveObjectReference {
                        obj_num: object_number,
                    })?
                    .try_stream(objects)
                    .map_err(PdfReaderError::ObjectError)
                    .and_then(|stream| read_object_stream(stream, objects));

                match result {
                    Ok(compressed) => {
                        insert_compressed_objects(
                            compressed,
                            source_offset,
                            objects,
                            source_offsets,
                        );
                        made_progress = true;
                    }
                    Err(error) => {
                        if first_error.is_none() {
                            first_error = Some(error);
                        }
                        deferred.push((source_offset, object_number));
                    }
                }
            }

            if deferred.is_empty() {
                return Ok(());
            }
            if !made_progress {
                return match first_error {
                    Some(error) => Err(error),
                    None => Err(PdfReaderError::UnresolvedObjects {
                        count: deferred.len(),
                        first_offset: deferred
                            .iter()
                            .map(|(offset, _)| *offset)
                            .min()
                            .unwrap_or(0),
                    }),
                };
            }
            pending = deferred;
        }

        Ok(())
    }
}

/// Returns ordinary object streams ordered by their physical declarations.
fn object_streams_in_physical_order(
    objects: &ObjectCollection,
    source_offsets: &HashMap<usize, usize>,
) -> Vec<(usize, usize)> {
    let mut streams = objects
        .map
        .iter()
        .filter_map(|(&object_number, object)| {
            let ObjectVariant::Stream(stream) = object else {
                return None;
            };
            let is_object_stream = matches!(
                stream.dictionary.get(b"Type"),
                Some(ObjectVariant::Name(name)) if name.as_slice() == b"ObjStm"
            );
            is_object_stream
                .then(|| source_offsets.get(&object_number).copied())
                .flatten()
                .map(|offset| (offset, object_number))
        })
        .collect::<Vec<_>>();
    streams.sort_unstable();
    streams
}

/// Inserts compressed values unless a physically newer declaration already won.
fn insert_compressed_objects(
    compressed: Vec<crate::object_stream::CompressedObject>,
    source_offset: usize,
    objects: &mut ObjectCollection,
    source_offsets: &mut HashMap<usize, usize>,
) {
    for object in compressed {
        if source_offsets
            .get(&object.number)
            .is_some_and(|existing_offset| *existing_offset > source_offset)
        {
            continue;
        }
        objects.insert_compressed(object.number, object.value);
        let _ = source_offsets.insert(object.number, source_offset);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_physical_object_and_keeps_the_latest_declaration() {
        let input = b"%PDF-1.7\n1 0 obj\n<< /Version /Old >>\nendobj\n1 2 obj\n<< /Version /New >>\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n";
        let mut diagnostics = Vec::new();
        let loaded = LinearObjectLoader::new(input, b"", &mut diagnostics)
            .load()
            .unwrap();
        let root = loaded.objects.get(1).expect("root object should load");

        assert!(matches!(
            root.try_dictionary(&loaded.objects)
                .unwrap()
                .get(b"Version"),
            Some(ObjectVariant::Name(version)) if version.as_slice() == b"New"
        ));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn resolves_a_forward_stream_length_through_existing_stream_recovery() {
        let payload = b"Hello world";
        let input = format!(
            "%PDF-1.7\n1 0 obj\n<< /Length 2 0 R >>\nstream\n{}\nendstream\nendobj\n2 0 obj\n{}\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n",
            String::from_utf8_lossy(payload),
            payload.len()
        );
        let mut diagnostics = Vec::new();
        let loaded = LinearObjectLoader::new(input.as_bytes(), b"", &mut diagnostics)
            .load()
            .unwrap();
        let stream = loaded
            .objects
            .get(1)
            .expect("stream should load")
            .try_stream(&loaded.objects)
            .unwrap();

        assert_eq!(stream.raw_data(), payload);
    }

    #[test]
    fn ignores_object_syntax_inside_a_stream() {
        let payload = b"99 0 obj\nnull\nendobj";
        let input = format!(
            "%PDF-1.7\n1 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n",
            payload.len(),
            String::from_utf8_lossy(payload)
        );
        let mut diagnostics = Vec::new();
        let loaded = LinearObjectLoader::new(input.as_bytes(), b"", &mut diagnostics)
            .load()
            .unwrap();

        assert!(loaded.objects.get(1).is_some());
        assert!(loaded.objects.get(99).is_none());
    }

    #[test]
    fn continues_after_eof_and_uses_the_newest_rooted_trailer() {
        let input = concat!(
            "%PDF-1.7\n",
            "% 99 0 obj null endobj\n",
            "1 0 obj\nnull\nendobj\n",
            "trailer\n<< /Root 1 0 R >>\n%%EOF\n",
            "2 0 obj\nnull\nendobj\n",
            "trailer\n<< /Root 2 0 R >>\n%%EOF\n"
        )
        .as_bytes();
        let mut diagnostics = Vec::new();
        let loaded = LinearObjectLoader::new(input, b"", &mut diagnostics)
            .load()
            .unwrap();

        assert_eq!(
            loaded.trailer.dictionary.get(b"Root"),
            Some(&ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(2, 0)))
        );
        assert!(loaded.objects.get(1).is_some());
        assert!(loaded.objects.get(2).is_some());
        assert!(loaded.objects.get(99).is_none());
    }

    #[test]
    fn fails_at_a_recognized_malformed_object() {
        let input = b"%PDF-1.7\n1 0 obj\n<< /Type\n";
        let mut diagnostics = Vec::new();
        let error = LinearObjectLoader::new(input, b"", &mut diagnostics)
            .load()
            .err()
            .expect("malformed object should stop the physical reader");

        assert!(matches!(error, PdfReaderError::ParserError(_)));
    }
}
