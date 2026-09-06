use pdf_object_reader::{
    dictionary::Dictionary, object_error::ObjectError, object_lookup::ObjectLookupExt,
    object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::AnnotationError;

/// A destination.
pub enum AnnotationDestination {
    /// A named destination.
    Named { name: Vec<u8> },
    /// An explicit destination array.
    Explicit(ExplicitDestination),
}

/// An explicit destination target.
pub enum DestinationTarget {
    /// A destination page dictionary.
    Dictionary(Dictionary),
    /// A destination page reference.
    Reference(pdf_object_reader::object_id::ObjectId),
}

/// An explicit destination array.
#[allow(clippy::large_enum_variant)]
pub enum ExplicitDestination {
    /// `/XYZ` destination.
    Xyz {
        /// The page target.
        page: DestinationTarget,
        /// Left coordinate.
        left: Option<f32>,
        /// Top coordinate.
        top: Option<f32>,
        /// Zoom factor.
        zoom: Option<f32>,
    },
    /// `/Fit` destination.
    Fit {
        /// The page target.
        page: DestinationTarget,
    },
    /// `/FitH` destination.
    FitH {
        /// The page target.
        page: DestinationTarget,
        /// Top coordinate.
        top: Option<f32>,
    },
    /// `/FitV` destination.
    FitV {
        /// The page target.
        page: DestinationTarget,
        /// Left coordinate.
        left: Option<f32>,
    },
    /// `/FitR` destination.
    FitR {
        /// The page target.
        page: DestinationTarget,
        /// Left coordinate.
        left: f32,
        /// Bottom coordinate.
        bottom: f32,
        /// Right coordinate.
        right: f32,
        /// Top coordinate.
        top: f32,
    },
    /// `/FitB` destination.
    FitB {
        /// The page target.
        page: DestinationTarget,
    },
    /// `/FitBH` destination.
    FitBH {
        /// The page target.
        page: DestinationTarget,
        /// Top coordinate.
        top: Option<f32>,
    },
    /// `/FitBV` destination.
    FitBV {
        /// The page target.
        page: DestinationTarget,
        /// Left coordinate.
        left: Option<f32>,
    },
}

impl AnnotationDestination {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        key: &'static [u8],
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        dictionary
            .get(key)
            .map(|value| Self::from_object(value, key, objects))
            .transpose()
    }

    pub(crate) fn from_object(
        value: &ObjectVariant,
        entry: &'static [u8],
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        Ok(match value {
            ObjectVariant::LiteralString(bytes)
            | ObjectVariant::HexString(bytes)
            | ObjectVariant::Name(bytes) => Self::Named {
                name: bytes.clone(),
            },
            ObjectVariant::Reference(_)
            | ObjectVariant::Dictionary(_)
            | ObjectVariant::Stream(_)
            | ObjectVariant::Array(_) => {
                Self::Explicit(explicit_destination(value, entry, objects)?)
            }
            other => return Err(ObjectError::TypeMismatch("Destination", other.name()).into()),
        })
    }
}

fn explicit_destination(
    value: &ObjectVariant,
    entry: &'static [u8],
    objects: &dyn ObjectResolver,
) -> Result<ExplicitDestination, AnnotationError> {
    let items = value.try_array(objects)?;

    let Some(page_item) = items.first() else {
        return Err(AnnotationError::InvalidEntry {
            entry,
            reason: "destination array is missing the page target".to_owned(),
        });
    };
    let Some(mode_item) = items.get(1) else {
        return Err(AnnotationError::InvalidEntry {
            entry,
            reason: "destination array is missing the destination type".to_owned(),
        });
    };

    let page = destination_target(page_item, objects)?;

    match mode_item.try_bytes(objects)? {
        b"XYZ" => Ok(ExplicitDestination::Xyz {
            page,
            left: items.optional_number(2, objects)?,
            top: items.optional_number(3, objects)?,
            zoom: items.optional_number(4, objects)?,
        }),
        b"Fit" => Ok(ExplicitDestination::Fit { page }),
        b"FitH" => Ok(ExplicitDestination::FitH {
            page,
            top: items.optional_number(2, objects)?,
        }),
        b"FitV" => Ok(ExplicitDestination::FitV {
            page,
            left: items.optional_number(2, objects)?,
        }),
        b"FitR" => Ok(ExplicitDestination::FitR {
            page,
            left: items.required_number(2, objects)?,
            bottom: items.required_number(3, objects)?,
            right: items.required_number(4, objects)?,
            top: items.required_number(5, objects)?,
        }),
        b"FitB" => Ok(ExplicitDestination::FitB { page }),
        b"FitBH" => Ok(ExplicitDestination::FitBH {
            page,
            top: items.optional_number(2, objects)?,
        }),
        b"FitBV" => Ok(ExplicitDestination::FitBV {
            page,
            left: items.optional_number(2, objects)?,
        }),
        other => Err(AnnotationError::InvalidEntry {
            entry,
            reason: format!("unsupported destination type '{other:?}'"),
        }),
    }
}

fn destination_target(
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<DestinationTarget, AnnotationError> {
    Ok(match value {
        ObjectVariant::Reference(object_number) => DestinationTarget::Reference(*object_number),
        _ => DestinationTarget::Dictionary(value.try_dictionary(objects)?.clone()),
    })
}
