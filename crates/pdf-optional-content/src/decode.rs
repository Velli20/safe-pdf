//! Decodes optional content declarations into owned data.
//!
//! A group is identified by the object number of its dictionary, so group references are
//! resolved only to read that number and the group's `/Name`.
use crate::error::OptionalContentError;
use crate::optional_content::{
    OptionalContentBaseState, OptionalContentConfiguration, OptionalContentGroup,
    OptionalContentGroupId, OptionalContentMembership, OptionalContentPolicy,
    OptionalContentProperties,
};
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};
use std::sync::Arc;

/// Decodes the catalog's `/OCProperties`; alternate `/Configs` are not retained.
impl FromPdfObject for OptionalContentProperties {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        Ok(Self {
            groups: context.optional_list(b"OCGs")?,
            default_configuration: context.optional(b"D")?.unwrap_or_default(),
        })
    }
}

/// Decodes the `/D` default configuration.
impl FromPdfObject for OptionalContentConfiguration {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        Ok(Self {
            base_state: context.optional(b"BaseState")?.unwrap_or_default(),
            on: context.optional_list(b"ON")?,
            off: context.optional_list(b"OFF")?,
        })
    }
}

/// Reads a `/BaseState` name. An absent entry is the caller's `On` default.
impl FromPdfObject for OptionalContentBaseState {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object().try_bytes(context.source())? {
            b"ON" => Ok(Self::On),
            b"OFF" => Ok(Self::Off),
            other => Err(OptionalContentError::InvalidEntry {
                entry: b"BaseState",
                reason: format!("unsupported optional content base state '{other:?}'"),
            }
            .into()),
        }
    }
}

/// Reads a `/P` policy name. An absent entry is the caller's `AnyOn` default.
impl FromPdfObject for OptionalContentPolicy {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object().try_bytes(context.source())? {
            b"AnyOn" => Ok(Self::AnyOn),
            b"AllOn" => Ok(Self::AllOn),
            b"AnyOff" => Ok(Self::AnyOff),
            b"AllOff" => Ok(Self::AllOff),
            other => Err(OptionalContentError::InvalidEntry {
                entry: b"P",
                reason: format!("unsupported optional content policy '{other:?}'"),
            }
            .into()),
        }
    }
}

/// Decodes one group dictionary, identified by its object number or, for a direct
/// dictionary, by its `/Name`.
impl FromPdfObject for OptionalContentGroup {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        // A malformed name never discards an otherwise identifiable group.
        let name = context.lenient::<Arc<[u8]>>(b"Name");
        Ok(Self {
            id: OptionalContentGroupId::new(context.dictionary().object_number, name.as_deref())?,
            name: name.map(|name| name.to_vec()),
        })
    }
}

/// Identifies one group dictionary by its object number or, for a direct dictionary,
/// by its `/Name`.
impl FromPdfObject for OptionalContentGroupId {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let name = context.lenient::<Arc<[u8]>>(b"Name");
        Ok(Self::new(
            context.dictionary().object_number,
            name.as_deref(),
        )?)
    }
}

/// Decodes an `/OCMD` or `/OCG` dictionary, such as an annotation's `/OC` entry or a
/// content stream's `/Properties` resource entry naming the optional content a
/// marked-content sequence belongs to.
impl FromPdfObject for OptionalContentMembership {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        match context.lenient::<Arc<[u8]>>(b"Type").as_deref() {
            Some(b"OCMD") => Ok(Self {
                // `/OCGs` is a single group or an array of them.
                groups: context.optional_list(b"OCGs")?,
                policy: context.optional(b"P")?.unwrap_or_default(),
                has_visibility_expression: context.dictionary().get(b"VE").is_some(),
            }),
            // A single group is a membership of that group under the default `AnyOn` policy.
            // A missing or invalid `/Type` is ignored and the dictionary read as a group.
            _ => {
                let name = context.lenient::<Arc<[u8]>>(b"Name");
                Ok(Self {
                    groups: vec![OptionalContentGroupId::new(
                        context.dictionary().object_number,
                        name.as_deref(),
                    )?],
                    policy: OptionalContentPolicy::AnyOn,
                    has_visibility_expression: false,
                })
            }
        }
    }
}
