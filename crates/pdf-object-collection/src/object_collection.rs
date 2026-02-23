use pdf_object::indirect_object::IndirectObject;
use pdf_object::object_resolver::ObjectResolver;
use pdf_object::{error::ObjectError, object_variant::ObjectVariant};
use std::collections::HashMap;

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
        // A limit to prevent infinite loops when resolving an object reference
        const MAX_DEREF: usize = 16;

        let mut current_obj = obj;

        for _ in 0..MAX_DEREF {
            match current_obj {
                ObjectVariant::Reference(object_number) => {
                    if let Some(obj) = self.map.get(object_number) {
                        current_obj = obj;
                    } else {
                        return Err(ObjectError::FailedResolveObjectReference {
                            obj_num: *object_number,
                        });
                    }
                }
                other => return Ok(other),
            }
        }

        // If we reach here, we exceeded MAX_DEREF without resolving to a non-reference.
        if let ObjectVariant::Reference(object_number) = current_obj {
            return Err(ObjectError::FailedResolveObjectReference {
                obj_num: *object_number,
            });
        }

        Ok(obj)
    }
}

impl ObjectCollection {
    /// Inserts a PDF object into the collection.
    ///
    /// This method handles different object variants and stores them using their
    /// appropriate keys:
    /// - `IndirectObject`: Stored by its `object_number`, with the inner object extracted.
    /// - `Stream`: Stored by its `object_number`.
    /// - `Reference`: Stored by the referenced object number.
    /// - Other variants are ignored and not stored.
    ///
    /// # Parameters
    ///
    /// - `obj`: The [`ObjectVariant`] to insert into the collection.
    ///
    /// # Returns
    ///
    /// An error if a duplicate key is detected otherwise `Ok(())`.
    pub fn insert(&mut self, obj: ObjectVariant) -> Result<(), ObjectError> {
        match obj {
            ObjectVariant::IndirectObject(indirect) => {
                let IndirectObject {
                    object_number,
                    object,
                    ..
                } = *indirect;
                let Some(object) = object else {
                    return Ok(());
                };

                if self.map.insert(object_number, object).is_some() {
                    return Err(ObjectError::DuplicateKeyInObjectCollection(object_number));
                }
            }
            ObjectVariant::Stream(ref stream) => {
                let key = stream.object_number;
                if self.map.insert(key, obj).is_some() {
                    return Err(ObjectError::DuplicateKeyInObjectCollection(key));
                }
            }
            ObjectVariant::Reference(ref reference) => {
                if self.map.insert(*reference, obj.clone()).is_some() {
                    return Err(ObjectError::DuplicateKeyInObjectCollection(*reference));
                }
            }
            _ => {}
        }
        Ok(())
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
            ObjectVariant::Dictionary(dict) => Self::dictionary_to_json(dict.as_ref()),
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
                    let _ = write!(acc, "{:02x}", b);
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
                                "entry_type": format!("{:?}", v.entry_type)
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
            ObjectVariant::IndirectObject(indirect) => {
                json!({
                    "type": "IndirectObject",
                    "object_number": indirect.object_number,
                    "generation_number": indirect.generation_number,
                    "object": indirect.object.as_ref().map(Self::object_variant_to_json)
                })
            }
            ObjectVariant::Reference(obj_num) => {
                json!({ "type": "Reference", "object_number": obj_num })
            }
            ObjectVariant::Stream(stream) => {
                json!({
                    "type": "Stream",
                    "object_number": stream.object_number,
                    "generation_number": stream.generation_number,
                    "dictionary": Self::dictionary_to_json(stream.dictionary.as_ref()),
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
            map.insert(key.clone(), Self::object_variant_to_json(value));
        }
        json!({
            "type": "Dictionary",
            "entries": JsonValue::Object(map)
        })
    }
}
