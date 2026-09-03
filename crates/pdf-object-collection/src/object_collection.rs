use pdf_object::object_resolver::ObjectResolver;
use pdf_object::{error::ObjectError, object_id::PdfObjectId, object_variant::ObjectVariant};
use std::collections::{HashMap, HashSet};

#[cfg(feature = "json")]
use serde_json::{Value as JsonValue, json};

#[derive(Default)]
pub struct ObjectCollection {
    pub map: HashMap<usize, ObjectVariant>,
}

impl ObjectResolver for ObjectCollection {
    fn resolve_object<'a>(
        &'a self,
        obj: &'a ObjectVariant,
    ) -> Result<&'a ObjectVariant, ObjectError> {
        let mut current_obj = obj;
        let mut in_progress = HashSet::new();

        loop {
            match current_obj {
                ObjectVariant::Reference(object_number) => {
                    if !in_progress.insert(*object_number) {
                        return Err(ObjectError::CyclicDependency {
                            obj_num: *object_number,
                        });
                    }
                    current_obj = self.map.get(object_number).ok_or(
                        ObjectError::FailedResolveObjectReference {
                            obj_num: *object_number,
                        },
                    )?;
                }
                other => return Ok(other),
            }
        }
    }
}

impl ObjectCollection {
    /// Creates an empty object collection with space for at least `capacity` objects.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
        }
    }

    /// Inserts a PDF object into the collection.
    ///
    /// The explicit identifier is authoritative. It is copied into dictionaries
    /// and streams whose runtime behavior needs the containing object number.
    ///
    /// # Parameters
    ///
    /// - `identifier`: The identifier parsed from the indirect object header.
    /// - `obj`: The direct [`ObjectVariant`] value to store.
    ///
    /// # Returns
    ///
    /// An error if a duplicate key is detected otherwise `Ok(())`.
    pub fn insert(
        &mut self,
        identifier: PdfObjectId,
        mut obj: ObjectVariant,
    ) -> Result<(), ObjectError> {
        match &mut obj {
            ObjectVariant::Dictionary(dictionary) => {
                dictionary.object_number = Some(identifier.number);
            }
            ObjectVariant::Stream(stream) => {
                stream.object_number = identifier.number;
                stream.generation_number = identifier.generation;
                if !stream.filters_applied()
                    && let Ok(data) = pdf_filter::filter::decode_with_resolver(stream, self)
                {
                    stream.set_filtered_data(data);
                }
            }
            _ => {}
        }

        if self.map.insert(identifier.number, obj).is_some() {
            return Err(ObjectError::DuplicateKeyInObjectCollection(
                identifier.number,
            ));
        }
        Ok(())
    }

    /// Inserts a compressed object (from an object stream) into the collection,
    /// overwriting any existing entry for `obj_num`.
    ///
    /// Unlike [`insert`](Self::insert), this method does not error on duplicate keys
    /// because compressed-object pass-2 loading may encounter object numbers that
    /// were already registered in pass 1 (e.g., via an object stream entry in the
    /// xref table). The object-stream version is authoritative for these objects.
    pub fn insert_compressed(&mut self, obj_num: usize, obj: ObjectVariant) {
        self.map.insert(obj_num, obj);
    }

    pub fn get(&self, key: usize) -> Option<&ObjectVariant> {
        if let Some(obj) = self.map.get(&key) {
            return Some(obj);
        }
        None
    }

    /// Serializes the `ObjectCollection` to a JSON string.
    ///
    /// This method is intended for debugging and testing purposes.
    /// It converts all objects in the collection to a JSON representation,
    /// preserving the structure and relationships between objects.
    ///
    /// # Returns
    ///
    /// A `Result` containing the JSON string representation of the collection,
    /// or a `serde_json::Error` if serialization fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let collection = ObjectCollection::default();
    /// let json = collection.to_json()?;
    /// println!("{}", json);
    /// ```
    #[cfg(feature = "json")]
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let json_value = self.to_json_value();
        serde_json::to_string_pretty(&json_value)
    }

    /// Serializes the `ObjectCollection` to a `serde_json::Value`.
    ///
    /// This method is useful when you need to manipulate the JSON structure
    /// before converting it to a string.
    ///
    /// # Returns
    ///
    /// A `serde_json::Value` representing the collection.
    #[cfg(feature = "json")]
    pub fn to_json_value(&self) -> JsonValue {
        let mut objects = serde_json::Map::new();
        for (key, value) in &self.map {
            objects.insert(key.to_string(), Self::object_variant_to_json(value));
        }
        JsonValue::Object(objects)
    }

    /// Converts an `ObjectVariant` to a `serde_json::Value`.
    #[cfg(feature = "json")]
    fn object_variant_to_json(obj: &ObjectVariant) -> JsonValue {
        match obj {
            ObjectVariant::Dictionary(dict) => Self::dictionary_to_json(dict),
            ObjectVariant::Array(arr) => {
                JsonValue::Array(arr.iter().map(Self::object_variant_to_json).collect())
            }
            ObjectVariant::LiteralString(s) => {
                json!({ "type": "LiteralString", "value": String::from_utf8_lossy(s).as_ref() })
            }
            ObjectVariant::Name(name) => {
                json!({ "type": "Name", "value": String::from_utf8_lossy(name).as_ref() })
            }
            ObjectVariant::Integer(i) => JsonValue::Number((*i).into()),
            ObjectVariant::Real(r) => serde_json::Number::from_f64(*r)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null),
            ObjectVariant::Boolean(b) => JsonValue::Bool(*b),
            ObjectVariant::Null => JsonValue::Null,
            ObjectVariant::HexString(bytes) => {
                // Encode hex string as base64 for JSON compatibility
                use std::fmt::Write;
                let hex: String = bytes.iter().fold(String::new(), |mut acc, b| {
                    let _ = write!(acc, "{b:02x}");
                    acc
                });
                json!({ "type": "HexString", "value": hex })
            }
            ObjectVariant::Trailer(trailer) => {
                json!({
                    "type": "Trailer",
                    "dictionary": Self::dictionary_to_json(trailer.dictionary.as_ref()),
                    "offset": trailer.offset
                })
            }
            ObjectVariant::CrossReferenceTable(xref) => {
                let entries: serde_json::Map<String, JsonValue> = xref
                    .entries
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.to_string(),
                            json!({
                                "entry_type": format!("{v:?}")
                            }),
                        )
                    })
                    .collect();
                json!({
                    "type": "CrossReferenceTable",
                    "entries": JsonValue::Object(entries),
                    "trailer": Self::object_variant_to_json(&ObjectVariant::Trailer(xref.trailer.clone()))
                })
            }
            ObjectVariant::EndOfFile => json!({ "type": "EndOfFile" }),
            ObjectVariant::Reference(obj_num) => {
                json!({ "type": "Reference", "object_number": obj_num })
            }
            ObjectVariant::Stream(stream) => {
                json!({
                    "type": "Stream",
                    "object_number": stream.object_number,
                    "generation_number": stream.generation_number,
                    "dictionary": Self::dictionary_to_json(&stream.dictionary),
                    "data_length": stream.raw_data().len()
                })
            }
        }
    }

    /// Converts a `Dictionary` to a `serde_json::Value`.
    #[cfg(feature = "json")]
    fn dictionary_to_json(dict: &pdf_object::dictionary::Dictionary) -> JsonValue {
        let mut map = serde_json::Map::new();
        for (key, value) in &dict.dictionary {
            map.insert(
                String::from_utf8_lossy(key).into_owned(),
                Self::object_variant_to_json(value),
            );
        }
        json!({
            "type": "Dictionary",
            "entries": JsonValue::Object(map)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{dictionary::Dictionary, stream::StreamObject};

    use super::*;

    #[test]
    fn inserting_unfiltered_stream_preserves_the_byte_allocation() {
        let data = vec![1, 2, 3, 4];
        let original = data.as_ptr();
        let stream = StreamObject::new(
            99,
            4,
            Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new()),
            data,
        );
        let mut collection = ObjectCollection::default();

        collection
            .insert(
                PdfObjectId {
                    number: 1,
                    generation: 0,
                },
                ObjectVariant::Stream(stream),
            )
            .expect("stream insert failed");

        let stored = collection
            .get(1)
            .and_then(|object| match object {
                ObjectVariant::Stream(stream) => Some(stream),
                _ => None,
            })
            .expect("inserted stream should be present");
        assert_eq!(stored.raw_data().as_ptr(), original);
        assert_eq!(stored.object_number, 1);
        assert_eq!(stored.generation_number, 0);
    }

    #[test]
    fn insertion_records_the_explicit_id_on_dictionaries() {
        let mut collection = ObjectCollection::default();
        collection
            .insert(
                PdfObjectId {
                    number: 7,
                    generation: 2,
                },
                ObjectVariant::Dictionary(Dictionary::new(
                    BTreeMap::<Vec<u8>, ObjectVariant>::new(),
                )),
            )
            .expect("dictionary insert failed");

        let stored = collection
            .get(7)
            .and_then(|object| match object {
                ObjectVariant::Dictionary(dictionary) => Some(dictionary),
                _ => None,
            })
            .expect("inserted dictionary should be present");
        assert_eq!(stored.object_number, Some(7));
    }

    #[test]
    fn resolve_object_reports_direct_self_reference_cycle() {
        let mut collection = ObjectCollection::default();
        collection.map.insert(1, ObjectVariant::Reference(1));

        let err = collection
            .resolve_object(&ObjectVariant::Reference(1))
            .expect_err("self-referential object should report a cycle");

        assert_eq!(err, ObjectError::CyclicDependency { obj_num: 1 });
    }

    #[test]
    fn resolve_object_reports_indirect_reference_cycle() {
        let mut collection = ObjectCollection::default();
        collection.map.insert(1, ObjectVariant::Reference(2));
        collection.map.insert(2, ObjectVariant::Reference(3));
        collection.map.insert(3, ObjectVariant::Reference(1));

        let err = collection
            .resolve_object(&ObjectVariant::Reference(1))
            .expect_err("mutually recursive references should report a cycle");

        assert_eq!(err, ObjectError::CyclicDependency { obj_num: 1 });
    }

    #[test]
    fn insert_decodes_stream_with_indirect_decode_parms() {
        use std::io::Write;

        let mut encoded_row = Vec::from([2u8]);
        encoded_row.extend_from_slice(b"hello");

        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&encoded_row).expect("zlib write failed");
        let compressed = encoder.finish().expect("zlib finish failed");

        let mut decode_parms = BTreeMap::new();
        decode_parms.insert(Vec::from(b"Predictor"), ObjectVariant::Integer(12));
        decode_parms.insert(Vec::from(b"Columns"), ObjectVariant::Integer(5));
        decode_parms.insert(Vec::from(b"Colors"), ObjectVariant::Integer(1));
        decode_parms.insert(Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8));

        let decode_parms_object = ObjectVariant::Dictionary(Dictionary::new(decode_parms));

        let mut collection = ObjectCollection::default();
        collection
            .insert(
                PdfObjectId {
                    number: 2,
                    generation: 0,
                },
                decode_parms_object,
            )
            .expect("decode parms insert failed");

        let mut stream_dict = BTreeMap::new();
        stream_dict.insert(
            Vec::from(b"Filter"),
            ObjectVariant::Name(b"FlateDecode".to_vec()),
        );
        stream_dict.insert(Vec::from(b"DecodeParms"), ObjectVariant::Reference(2));
        stream_dict.insert(
            Vec::from(b"Length"),
            ObjectVariant::Integer(compressed.len() as i64),
        );

        let stream = ObjectVariant::Stream(StreamObject::new_encoded(
            1,
            0,
            Dictionary::new(stream_dict),
            compressed,
        ));

        collection
            .insert(
                PdfObjectId {
                    number: 1,
                    generation: 0,
                },
                stream,
            )
            .expect("stream insert failed");

        let decoded = collection.get(1).and_then(|obj| match obj {
            ObjectVariant::Stream(stream) => Some((stream.raw_data(), stream.filters_applied())),
            _ => None,
        });

        assert_eq!(decoded, Some((b"hello".as_slice(), true)));
    }

    #[test]
    fn insert_preserves_encoded_state_when_filter_dependencies_are_unresolved() {
        let stream_dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"DecodeParms"), ObjectVariant::Reference(2)),
            (
                Vec::from(b"Filter"),
                ObjectVariant::Name(b"ASCIIHexDecode".to_vec()),
            ),
        ]));
        let stream = StreamObject::new_encoded(1, 0, stream_dictionary, b"2A>".to_vec());
        let mut collection = ObjectCollection::default();

        collection
            .insert(
                PdfObjectId {
                    number: 1,
                    generation: 0,
                },
                ObjectVariant::Stream(stream),
            )
            .expect("encoded stream insertion should be recoverable");
        let stored = collection
            .get(1)
            .and_then(|object| match object {
                ObjectVariant::Stream(stream) => Some(stream),
                _ => None,
            })
            .expect("stream should remain in the collection");
        assert!(!stored.filters_applied());
        assert_eq!(stored.raw_data(), b"2A>");

        collection
            .insert(
                PdfObjectId {
                    number: 2,
                    generation: 0,
                },
                ObjectVariant::Dictionary(Dictionary::new(
                    BTreeMap::<Vec<u8>, ObjectVariant>::new(),
                )),
            )
            .expect("decode parameters should be inserted");

        let stored = collection
            .get(1)
            .and_then(|object| match object {
                ObjectVariant::Stream(stream) => Some(stream),
                _ => None,
            })
            .expect("stream should remain in the collection");
        let decoded = pdf_filter::filter::decode_with_resolver(stored, &collection)
            .expect("filter retry should succeed after dependency insertion");
        assert_eq!(decoded.as_ref(), &[0x2A]);
    }

    #[test]
    fn with_capacity_preallocates_the_object_map() {
        let collection = ObjectCollection::with_capacity(12);

        assert!(collection.map.capacity() >= 12);
        assert!(collection.map.is_empty());
    }
}
