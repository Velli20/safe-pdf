//! Decodes optional content declarations into owned Core data.
//!
//! Group resources are never fetched beyond the `/Name` needed to identify a group
//! that has no object number, preserving reference identity the way
//! [`crate::ocg_state`] does for `SetOCGState` targets.
use crate::error::SourceDecodeError;
use crate::optional_content::{
    AnnotationOptionalContent, OptionalContentBaseState, OptionalContentConfiguration,
    OptionalContentGroup, OptionalContentGroupId, OptionalContentMembership, OptionalContentPolicy,
    OptionalContentProperties,
};
use crate::source_decode::DecodeResult;
use pdf_object_reader::{
    Dictionary, FromPdfObject, ObjectAccess, ObjectContext, ReadResult,
    object_lookup::ObjectLookupExt, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

impl FromPdfObject for OptionalContentProperties {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let context = context.dictionary()?;
        let dictionary = context.dictionary();
        let objects = context.source();

        let mut groups = Vec::new();
        if let Some(value) = dictionary.get(b"OCGs") {
            for item in value.try_array(objects)? {
                let Some(id) = group_id(item, objects)? else {
                    continue;
                };
                // An unreadable group dictionary still identifies a group; only its
                // name is lost, so the group is retained without one.
                let name = item
                    .try_dictionary(objects)
                    .ok()
                    .and_then(|group| group.optional_bytes_vec(b"Name", objects).ok())
                    .flatten();
                groups.push(OptionalContentGroup { id, name });
            }
        }

        let default_configuration = match dictionary.optional_dictionary(b"D", objects)? {
            Some(configuration) => configuration_from_dictionary(configuration, objects)?,
            None => OptionalContentConfiguration::default(),
        };

        Ok(Self {
            groups,
            default_configuration,
        })
    }
}

/// Decodes the `/D` default configuration; alternate `/Configs` are not retained.
fn configuration_from_dictionary(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> DecodeResult<OptionalContentConfiguration> {
    let base_state = match dictionary.optional_bytes(b"BaseState", objects)? {
        None | Some(b"ON") => OptionalContentBaseState::On,
        Some(b"OFF") => OptionalContentBaseState::Off,
        Some(other) => {
            return Err(SourceDecodeError::InvalidEntry {
                entry: b"BaseState",
                reason: format!("unsupported optional content base state '{other:?}'"),
            });
        }
    };
    Ok(OptionalContentConfiguration {
        base_state,
        on: group_list(dictionary, b"ON", objects)?,
        off: group_list(dictionary, b"OFF", objects)?,
    })
}

/// Decodes an optional array of group targets, dropping unidentifiable entries.
fn group_list(
    dictionary: &Dictionary,
    key: &'static [u8],
    objects: &dyn ObjectResolver,
) -> DecodeResult<Vec<OptionalContentGroupId>> {
    let Some(value) = dictionary.get(key) else {
        return Ok(Vec::new());
    };
    let mut ids = Vec::new();
    for item in value.try_array(objects)? {
        if let Some(id) = group_id(item, objects)? {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// Identifies one group, preferring its reference identity over its `/Name`.
///
/// Returns `None` for a null entry and for a direct dictionary with neither an
/// object number nor a name, which names no group this state can track.
fn group_id(
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> DecodeResult<Option<OptionalContentGroupId>> {
    if let ObjectVariant::Reference(id) = value {
        return Ok(Some(OptionalContentGroupId::Object {
            number: u64::try_from(id.number).map_err(|_| SourceDecodeError::ResourceLimit)?,
        }));
    }
    if value.is_null(objects)? {
        return Ok(None);
    }
    let group = value.try_dictionary(objects)?;
    if let Some(number) = group.object_number {
        return Ok(Some(OptionalContentGroupId::Object {
            number: u64::try_from(number).map_err(|_| SourceDecodeError::ResourceLimit)?,
        }));
    }
    Ok(group
        .optional_bytes_vec(b"Name", objects)?
        .map(|name| OptionalContentGroupId::Name { name }))
}

/// Decodes an annotation's `/OC` entry, absent when the annotation declares none.
///
/// A malformed entry fails the annotation, matching the strictness every other
/// entry in [`crate::source_decode`] applies to a value that is present.
pub(crate) fn optional_content(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> DecodeResult<Option<AnnotationOptionalContent>> {
    const ENTRY: &[u8] = b"OC";

    let Some(value) = dictionary.get(ENTRY) else {
        return Ok(None);
    };
    if value.is_null(objects)? {
        return Ok(None);
    }

    let content = dictionary.required_dictionary(ENTRY, objects)?;
    match content.optional_bytes(b"Type", objects)? {
        Some(b"OCMD") => Ok(Some(AnnotationOptionalContent::Membership(Box::new(
            membership(content, objects)?,
        )))),
        None | Some(b"OCG") => match group_id(value, objects)? {
            Some(id) => Ok(Some(AnnotationOptionalContent::Group(id))),
            None => Err(SourceDecodeError::MissingEntry { entry: b"Name" }),
        },
        Some(other) => Err(SourceDecodeError::InvalidEntry {
            entry: ENTRY,
            reason: format!("optional content has type '{other:?}'"),
        }),
    }
}

/// Decodes an `/OCMD` membership dictionary and its visibility policy.
fn membership(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> DecodeResult<OptionalContentMembership> {
    // `/OCGs` is a single group or an array of them; both forms are retained as a list.
    let groups = match dictionary.get(b"OCGs") {
        Some(value) if !value.is_null(objects)? => {
            if matches!(resolved(value, objects)?, ObjectVariant::Array(_)) {
                group_list(dictionary, b"OCGs", objects)?
            } else {
                group_id(value, objects)?.into_iter().collect()
            }
        }
        _ => Vec::new(),
    };

    let policy = dictionary
        .optional_bytes(b"P", objects)?
        .map(OptionalContentPolicy::try_from)
        .transpose()?
        .unwrap_or_default();

    Ok(OptionalContentMembership {
        groups,
        policy,
        has_visibility_expression: dictionary.get(b"VE").is_some(),
    })
}

/// Resolves one level of indirection so an entry's shape can be inspected.
fn resolved<'a>(
    value: &'a ObjectVariant,
    objects: &'a dyn ObjectResolver,
) -> Result<&'a ObjectVariant, SourceDecodeError> {
    match value {
        ObjectVariant::Reference(_) => Ok(objects.resolve_object(value)?),
        other => Ok(other),
    }
}
