//! Interpretation of retained source metadata, without mutation or parser access.
use crate::error::SourceAdoptionError;
use crate::fields::TextMode;
use crate::kind::SourceLayout;
use crate::pdf_data::*;
use pdf_graphics::rect::Rect;

/// Decode an optional PDF text string; absence yields an empty string.
///
/// Returns `InvalidTextString` for bytes without a Unicode interpretation.
pub(crate) fn decode_text(bytes: Option<&[u8]>) -> Result<String, SourceAdoptionError> {
    Ok(pdf_object_reader::text_string::decode(
        bytes.unwrap_or_default(),
    )?)
}

impl BorderStyle {
    /// Resolve retained `/W`: absent means one page unit and negative means zero.
    /// Does not mutate metadata or panic.
    pub fn resolved_width(&self) -> f32 {
        self.width.unwrap_or(1.0).max(0.0)
    }
}

impl SourceAnnotation {
    /// The widget record, when this is a widget annotation.
    pub(crate) fn widget(&self) -> Option<&WidgetAnnotation> {
        match &self.kind {
            NativeAnnotation::Widget(widget) => Some(widget),
            _ => None,
        }
    }

    /// Decode the retained `/Contents`; absent contents yield an empty string.
    pub fn contents(&self) -> Result<String, SourceAdoptionError> {
        decode_text(self.contents.as_deref())
    }

    /// Normalize the source `/Rect` into validated bounds, whatever its edge order.
    ///
    /// Returns `MissingRect` when the record has no rectangle and `InvalidRect`
    /// when the normalized rectangle is nonfinite or empty. Does not panic.
    pub fn bounds(&self) -> Result<Rect, SourceAdoptionError> {
        let rect = self
            .rect
            .ok_or(SourceAdoptionError::MissingRect { source_id: self.id })?
            .normalized();
        if !rect.is_valid() {
            return Err(SourceAdoptionError::InvalidRect { source_id: self.id });
        }
        Ok(rect)
    }

    /// Borrow the retained action of a link or widget; other subtypes carry none.
    pub fn action(&self) -> Option<&AnnotationAction> {
        match &self.kind {
            NativeAnnotation::Link(v) => v.action.as_ref(),
            NativeAnnotation::Widget(v) => v.action.as_ref(),
            _ => None,
        }
    }

    /// Normalized `/Rect` with the page point it was measured against: the first
    /// ink point, else the rectangle's origin. `None` when [`Self::bounds`] fails.
    pub fn layout(&self) -> Option<SourceLayout> {
        let rect = self.bounds().ok()?;
        let first_ink_point = match &self.kind {
            NativeAnnotation::Ink(v) => v
                .ink_list
                .strokes
                .iter()
                .flat_map(|stroke| &stroke.points)
                .next()
                .copied(),
            _ => None,
        };
        let anchor = first_ink_point.unwrap_or_else(|| rect.origin());
        Some(SourceLayout { rect, anchor })
    }

    /// Resolve the source button's on-state without changing the live shared field.
    ///
    /// Prefer a current non-Off `/AS` present in normal appearance names; otherwise
    /// use the first non-Off name. Checkboxes without one use `Yes`. The caller must
    /// supply a button annotation. Returns `MissingButtonOnState` when no state can
    /// be resolved. This method borrows source bytes and does not panic.
    pub fn button_on_state(&self) -> Result<&[u8], SourceAdoptionError> {
        let states = self
            .appearance
            .as_ref()
            .map(|v| v.normal_states.as_slice())
            .unwrap_or_default();
        if let Some(state) = self
            .appearance_state
            .as_deref()
            .filter(|state| *state != b"Off" && states.iter().any(|name| name == state))
        {
            return Ok(state);
        }
        if let Some(state) = states.iter().find(|state| state.as_slice() != b"Off") {
            return Ok(state);
        }
        if matches!(&self.kind, NativeAnnotation::Widget(widget) if widget.is_checkbox()) {
            return Ok(b"Yes");
        }
        Err(SourceAdoptionError::MissingButtonOnState { source_id: self.id })
    }
}

impl WidgetAnnotation {
    /// Whether retained `/FT` is `Btn`. Does not inspect flags or panic.
    pub fn is_button(&self) -> bool {
        self.field_type.as_deref() == Some(b"Btn")
    }

    /// Whether retained type and flags describe a checkbox. Does not panic.
    pub fn is_checkbox(&self) -> bool {
        self.is_button() && !self.is_radio_button() && !self.is_push_button()
    }

    /// Whether retained `/FT` and flags describe a listbox. Does not panic.
    pub fn is_listbox(&self) -> bool {
        self.field_type.as_deref() == Some(b"Ch") && !self.is_combo_box()
    }

    /// Borrow `annotation`'s active source appearance name, excluding `Off`.
    /// Does not infer a normalized live field value, validate the type, or panic.
    pub fn active_button_state<'a>(&self, annotation: &'a SourceAnnotation) -> Option<&'a [u8]> {
        annotation
            .appearance_state
            .as_deref()
            .filter(|state| *state != b"Off")
    }

    /// Text entry mode from the retained flags; password takes precedence over multiline.
    pub fn text_mode(&self) -> TextMode {
        let flags = self.field_flags.unwrap_or_default();
        if flags.contains(WidgetFieldFlags::PASSWORD) {
            TextMode::Password
        } else if flags.contains(WidgetFieldFlags::MULTILINE) {
            TextMode::Multiline
        } else {
            TextMode::SingleLine
        }
    }

    /// Whether the retained radio flag is set; callers check field type separately.
    /// This read-only query does not panic.
    pub fn is_radio_button(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::RADIO_BUTTON))
    }

    /// Whether the retained push button flag is set; callers check field type separately.
    /// This read-only query does not panic.
    pub fn is_push_button(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::PUSH_BUTTON))
    }

    /// Whether the retained combo flag is set; callers check field type separately.
    /// This read-only query does not panic.
    pub fn is_combo_box(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::COMBO_BOX))
    }

    /// Whether the retained multiple selection flag is set; callers check field type separately.
    /// This read-only query does not panic.
    pub fn is_multi_select(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::MULTI_SELECT))
    }

    /// Whether the retained read-only flag is set; callers check field type separately.
    /// This read-only query does not panic.
    pub fn is_read_only(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::READ_ONLY))
    }

    /// Whether the retained no-toggle-to-off flag is set; callers check field type separately.
    /// This read-only query does not panic.
    pub fn is_no_toggle_to_off(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::NO_TOGGLE_TO_OFF))
    }

    /// Whether the retained radios-in-unison flag is set; callers check field type separately.
    /// This read-only query does not panic.
    pub fn is_radios_in_unison(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::RADIOS_IN_UNISON))
    }

    /// Resolve source choice indices, preferring explicit `/I` over inferred `/V`.
    ///
    /// Inferred values use the first matching export value, then sort/deduplicate
    /// indices as the legacy reader does. Core value commands intentionally require
    /// ordered distinct option identities instead. Returns `IndexOverflow` if a
    /// platform index cannot fit u64; does not mutate state or panic.
    pub fn selected_option_indices(&self) -> Result<Vec<u64>, SourceAdoptionError> {
        if let Some(indices) = &self.selected_indices {
            return Ok(indices.clone());
        }
        let Some(options) = &self.options else {
            return Ok(Vec::new());
        };
        let values: &[WidgetFieldValue] = match &self.value {
            Some(WidgetFieldValue::Array(values)) => values,
            Some(value) => std::slice::from_ref(value),
            None => &[],
        };
        let mut indices = Vec::new();
        for value in values {
            if let WidgetFieldValue::Bytes(value) = value
                && let Some(index) = options
                    .iter()
                    .position(|option| option.export_value == *value)
            {
                indices.push(u64::try_from(index).map_err(|_| {
                    SourceAdoptionError::IndexOverflow {
                        field: "choice index",
                    }
                })?);
            }
        }
        indices.sort_unstable();
        indices.dedup();
        Ok(indices)
    }
}
