//! Retained source data is distinct from authoritative live annotation values.
use serde::{Deserialize, Serialize};

bitflags::bitflags! {
    /// PDF annotation flags from `/F`, shared by live and retained source metadata.
    /// Unknown bits are preserved through decoding and serialization.
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct AnnotationFlags: i32 {
        /// Hide annotations whose type is unsupported and which lack an appearance.
        const INVISIBLE = 1 << 0;
        /// Suppress display, printing, and interaction.
        const HIDDEN = 1 << 1;
        /// Include the annotation when printing.
        const PRINT = 1 << 2;
        /// Preserve physical size when the page is zoomed.
        const NO_ZOOM = 1 << 3;
        /// Preserve orientation when the page is rotated.
        const NO_ROTATE = 1 << 4;
        /// Suppress screen display and interaction.
        const NO_VIEW = 1 << 5;
        /// Prevent user interaction with the annotation.
        const READ_ONLY = 1 << 6;
        /// Prevent deletion and changes to position or size.
        const LOCKED = 1 << 7;
        /// Invert the no-view behavior for certain events.
        const TOGGLE_NO_VIEW = 1 << 8;
        /// Prevent changes to the annotation's contents.
        const LOCKED_CONTENTS = 1 << 9;
    }
}

impl Serialize for AnnotationFlags {
    /// Serializes the raw `/F` integer, including unknown bits.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i32(self.bits())
    }
}

impl<'de> Deserialize<'de> for AnnotationFlags {
    /// Deserializes the raw `/F` integer without discarding unknown bits.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        i32::deserialize(deserializer).map(Self::from_bits_retain)
    }
}

/// Common live capabilities plus an optional immutable source snapshot.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct AnnotationMetadata {
    /// Live PDF annotation flag bits, including unknown bits; zero is unrestricted.
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub flags: AnnotationFlags,
    /// Source snapshot for inspection and future export. Its values are historical,
    /// never a fallback that overrides current geometry, content, or field values.
    /// Source references belong to the containing document's PDF.
    pub source: Option<Box<crate::pdf_data::SourceAnnotation>>,
    /// Current host-owned action metadata; storing it never executes the action.
    pub action: Option<Box<crate::pdf_data::AnnotationAction>>,
}

impl AnnotationMetadata {
    /// Metadata adopting `source`: its flags, a retained snapshot, and its action.
    pub fn from_source(source: &crate::pdf_data::SourceAnnotation) -> Self {
        Self {
            flags: source.flags.unwrap_or_default(),
            source: Some(Box::new(source.clone())),
            action: source.action().cloned().map(Box::new),
        }
    }

    /// Whether live flags permit screen display. Does not mutate state or panic.
    pub fn is_visible(&self) -> bool {
        !self.flags.intersects(
            AnnotationFlags::INVISIBLE | AnnotationFlags::HIDDEN | AnnotationFlags::NO_VIEW,
        )
    }

    /// Whether flags permit interactive geometry changes for a visible annotation.
    /// Read-only and locked annotations reject movement. Does not panic.
    pub fn can_translate(&self) -> bool {
        self.is_visible()
            && !self
                .flags
                .intersects(AnnotationFlags::READ_ONLY | AnnotationFlags::LOCKED)
    }

    /// Whether flags permit visible content edits; locked-contents prevents them.
    /// The host must separately check the annotation's supported editor. Does not panic.
    pub fn can_edit_contents(&self) -> bool {
        self.is_visible()
            && !self
                .flags
                .intersects(AnnotationFlags::READ_ONLY | AnnotationFlags::LOCKED_CONTENTS)
    }

    /// Whether read-only or locked flags permit structural deletion.
    /// Visibility does not affect this authoring operation; does not panic.
    pub fn can_delete(&self) -> bool {
        !self
            .flags
            .intersects(AnnotationFlags::READ_ONLY | AnnotationFlags::LOCKED)
    }

    /// Whether the host should preserve physical size under zoom. Does not panic.
    pub fn no_zoom(&self) -> bool {
        self.flags.contains(AnnotationFlags::NO_ZOOM)
    }

    /// Whether the host should preserve orientation during page rotation. Does not panic.
    pub fn no_rotate(&self) -> bool {
        self.flags.contains(AnnotationFlags::NO_ROTATE)
    }

    pub(crate) fn validate(&self) -> Result<(), crate::error::ValidationError> {
        crate::validation::validate(self)?;
        Ok(())
    }
}

/// Historical PDF field state. Live normalized values remain in `FieldKind`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct FieldMetadata {
    /// Original terminal field/widget metadata, including defaults, raw names,
    /// flags, options, values, actions, and additional-action dictionaries.
    /// These values never overwrite subsequent Core value commands.
    pub source: Option<Box<crate::pdf_data::WidgetAnnotation>>,
}

/// A field whose value can be retained but has no supported Core value editor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct UnsupportedField {
    /// Source field type, including signature, push-button, or unknown types.
    pub field_type: Vec<u8>,
    /// Current retained value, distinct from an absent value.
    pub value: Option<crate::pdf_data::WidgetFieldValue>,
}
