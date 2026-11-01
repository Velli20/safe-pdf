//! Decodes optional content declarations into owned data.
//!
//! A group is identified by the object number of its dictionary, so group references are
//! resolved only to read that number and the group's `/Name`.
use crate::error::OptionalContentError;
use crate::ocg_state::{OcgOperation, OcgState, OcgStateChange, OcgTarget};
use crate::optional_content::{
    OptionalContentBaseState, OptionalContentConfiguration, OptionalContentGroup,
    OptionalContentGroupId, OptionalContentMembership, OptionalContentPolicy,
    OptionalContentProperties,
};
use pdf_object_reader::pdf_name::PdfName;
use pdf_object_reader::{ArrayContext, FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

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
        let name = context.decode::<PdfName>()?;
        match &*name {
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

/// Reads a `/State` operation name.
impl FromPdfObject for OcgOperation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let name = context.decode::<PdfName>()?;
        match &*name {
            b"ON" => Ok(Self::On),
            b"OFF" => Ok(Self::Off),
            b"Toggle" => Ok(Self::Toggle),
            other => Err(OptionalContentError::InvalidEntry {
                entry: b"State",
                reason: format!("unsupported optional content operation '{other:?}'"),
            }
            .into()),
        }
    }
}

/// Reads a `/P` policy name. An absent entry is the caller's `AnyOn` default.
impl FromPdfObject for OptionalContentPolicy {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let name = context.decode::<PdfName>()?;
        match &*name {
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
        let name = context.lenient::<PdfName>(b"Name");
        Ok(Self {
            id: OptionalContentGroupId::new(context.object_number(), name.as_deref())?,
            name: name.map(|name| name.to_vec()),
        })
    }
}

/// Identifies one group dictionary by its object number or, for a direct dictionary,
/// by its `/Name`.
impl FromPdfObject for OptionalContentGroupId {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let name = context.lenient::<PdfName>(b"Name");
        Ok(Self::new(context.object_number(), name.as_deref())?)
    }
}

/// Decodes an `/OCMD` or `/OCG` dictionary, such as an annotation's `/OC` entry or a
/// content stream's `/Properties` resource entry naming the optional content a
/// marked-content sequence belongs to.
impl FromPdfObject for OptionalContentMembership {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        match context.lenient::<PdfName>(b"Type").as_deref() {
            Some(b"OCMD") => Ok(Self {
                // `/OCGs` is a single group or an array of them.
                groups: context.optional_list(b"OCGs")?,
                policy: context.optional(b"P")?.unwrap_or_default(),
                // A malformed expression falls back to `/OCGs` and `/P`.
                visibility_expression: context.lenient(b"VE"),
            }),
            // A single group is a membership of that group under the default `AnyOn` policy.
            // A missing or invalid `/Type` is ignored and the dictionary read as a group.
            _ => {
                let name = context.lenient::<PdfName>(b"Name");
                Ok(Self {
                    groups: vec![OptionalContentGroupId::new(
                        context.object_number(),
                        name.as_deref(),
                    )?],
                    policy: OptionalContentPolicy::AnyOn,
                    visibility_expression: None,
                })
            }
        }
    }
}

/// Decodes a direct `/OCG` dictionary of a `SetOCGState` `/State` array, retaining its `/Name`.
///
/// Group references keep their identity unresolved and never reach this decoder.
impl FromPdfObject for OcgTarget {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let group_type = context.required::<PdfName>(b"Type")?;
        if group_type.as_ref() != b"OCG" {
            return Err(OptionalContentError::InvalidEntry {
                entry: b"State",
                reason: format!("optional content group has type '{group_type:?}'"),
            }
            .into());
        }
        Ok(Self::Dictionary {
            name: context.required(b"Name")?,
        })
    }
}

/// Decodes the `/State` array of a `SetOCGState` action.
///
/// Each operation name starts a change that governs the group targets following it, so
/// the array splits at its names; every change must govern at least one group.
impl FromPdfObject for OcgState {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut items = context.array()?;
        let mut start = 0;
        let changes = items
            .array()
            .chunk_by(|_, next| !next.is_name())
            .map(|chunk| {
                let change = OcgStateChange::read(&mut items, start, chunk.len());
                start = start.saturating_add(chunk.len());
                change
            })
            .collect::<ReadResult<_>>()?;
        Ok(Self { changes })
    }
}

impl OcgStateChange {
    /// Reads the operation at `start` and the `length - 1` group targets after it.
    fn read<A: ObjectAccess + ?Sized>(
        items: &mut ArrayContext<'_, A>,
        start: usize,
        length: usize,
    ) -> ReadResult<Self> {
        if !items.array().get(start).is_some_and(|item| item.is_name()) {
            return Err(OptionalContentError::InvalidEntry {
                entry: b"State",
                reason: "group target precedes any optional content operation".to_owned(),
            }
            .into());
        }
        if length < 2 {
            return Err(OptionalContentError::InvalidEntry {
                entry: b"State",
                reason: "optional content operation has no group target".to_owned(),
            }
            .into());
        }
        let end = start.saturating_add(length);
        Ok(Self {
            operation: items.at(start)?,
            groups: (start.saturating_add(1)..end)
                .map(|index| OcgTarget::read(items, index))
                .collect::<ReadResult<_>>()?,
        })
    }
}

impl OcgTarget {
    /// Reads the group target at `index`. A group reference keeps its identity
    /// unresolved; a direct dictionary is decoded for its `/Name`.
    fn read<A: ObjectAccess + ?Sized>(
        items: &mut ArrayContext<'_, A>,
        index: usize,
    ) -> ReadResult<Self> {
        let Some(id) = items.reference_at(index) else {
            return items.at(index);
        };
        Ok(Self::Reference {
            number: u64::try_from(id.number).map_err(|_| OptionalContentError::ResourceLimit)?,
            generation: u64::try_from(id.generation)
                .map_err(|_| OptionalContentError::ResourceLimit)?,
        })
    }
}
