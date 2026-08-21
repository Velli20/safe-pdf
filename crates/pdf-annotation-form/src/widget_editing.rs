use pdf_annotation_types::{
    Annotation, AnnotationKind, ButtonStateError, WidgetAnnotation, WidgetFieldValue,
    annotation_id::AnnotationId,
};
use pdf_document::document::PdfDocument;
use thiserror::Error;

/// Errors produced while editing widget annotations.
#[derive(Debug, Error, PartialEq)]
pub enum WidgetEditError {
    /// The requested page is not present in the document.
    #[error("page {page_index} was not found in this document")]
    PageNotFound { page_index: usize },
    /// The requested annotation is not present on the page.
    #[error("annotation {id} was not found on page {page_index}")]
    AnnotationNotFound { page_index: usize, id: usize },
    /// The requested annotation is not a widget annotation.
    #[error("annotation {id} has subtype /{subtype}, expected /Widget")]
    WrongSubtype { id: usize, subtype: String },
    /// The requested widget is not a button form field.
    #[error("annotation {id} is not a /Btn widget field")]
    NotButton { id: usize },
    /// The requested button has a different button kind.
    #[error("annotation {id} is not a {expected}")]
    WrongButtonKind { id: usize, expected: &'static str },
    /// A checked or selected state cannot be inferred from the normal appearance.
    #[error("annotation {id} has no usable non-/Off normal appearance state")]
    MissingButtonOnState { id: usize },
    /// The requested widget is not a listbox choice field.
    #[error("annotation {id} is not a listbox /Ch widget field")]
    NotListbox { id: usize },
    /// The listbox has no choice options.
    #[error("annotation {id} has no /Opt choice options")]
    MissingChoiceOptions { id: usize },
    /// A selected option index is outside the listbox options.
    #[error("option index {index} is outside the {option_count} options for annotation {id}")]
    ChoiceIndexOutOfBounds {
        id: usize,
        index: usize,
        option_count: usize,
    },
    /// A single-select listbox was given multiple selected indices.
    #[error("annotation {id} does not permit multiple selected options")]
    MultipleSelectionNotAllowed { id: usize },
    #[error("{0}")]
    ButtonStateError(#[from] ButtonStateError),
}

/// The result of activating a widget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WidgetActivation {
    /// Whether activation changed a persistent widget state.
    pub state_changed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WidgetLocation {
    page_index: usize,
    annotation_id: AnnotationId,
}

struct ButtonMember {
    location: WidgetLocation,
    on_state: Option<Vec<u8>>,
    active_state: Option<Vec<u8>>,
}

/// Edits widget annotations across a materialized PDF document.
pub struct WidgetEditor<'a> {
    document: &'a mut PdfDocument,
}

impl<'a> WidgetEditor<'a> {
    /// Creates an editor for widgets in `document`.
    pub fn new(document: &'a mut PdfDocument) -> Self {
        Self { document }
    }

    /// Activates a widget, returning `None` for non-button annotations.
    pub fn activate(
        &mut self,
        page_index: usize,
        id: AnnotationId,
    ) -> Result<Option<WidgetActivation>, WidgetEditError> {
        let (is_radio_button, active, no_toggle_to_off) = {
            let annotation = self.annotation(page_index, id)?;
            let AnnotationKind::Widget(widget) = &annotation.kind else {
                return Ok(None);
            };
            if !widget.is_button() {
                return Ok(None);
            }
            if widget.is_push_button() || widget.is_read_only() {
                return Ok(Some(WidgetActivation {
                    state_changed: false,
                }));
            }
            (
                widget.is_radio_button(),
                widget.active_button_state(annotation).is_some(),
                widget.is_no_toggle_to_off(),
            )
        };

        if is_radio_button && active && no_toggle_to_off {
            return Ok(Some(WidgetActivation {
                state_changed: false,
            }));
        }

        if is_radio_button {
            self.set_radio_selected(page_index, id, !active)?;
        } else {
            self.set_checkbox_checked(page_index, id, !active)?;
        }
        Ok(Some(WidgetActivation {
            state_changed: true,
        }))
    }

    /// Sets the checked state of a checkbox field.
    pub fn set_checkbox_checked(
        &mut self,
        page_index: usize,
        id: AnnotationId,
        checked: bool,
    ) -> Result<(), WidgetEditError> {
        self.set_button_selected(
            page_index,
            id,
            checked,
            WidgetAnnotation::is_checkbox,
            "checkbox",
        )
    }

    /// Sets whether a radio widget is the selected member of its field.
    pub fn set_radio_selected(
        &mut self,
        page_index: usize,
        id: AnnotationId,
        selected: bool,
    ) -> Result<(), WidgetEditError> {
        self.set_button_selected(
            page_index,
            id,
            selected,
            WidgetAnnotation::is_radio_button,
            "radio button",
        )
    }

    /// Activates one listbox option, replacing or toggling selection as appropriate.
    pub fn activate_listbox_option(
        &mut self,
        page_index: usize,
        id: AnnotationId,
        option_index: usize,
    ) -> Result<Option<WidgetActivation>, WidgetEditError> {
        let (mut selected, multi_select, read_only, option_count) = {
            let annotation = self.annotation(page_index, id)?;
            let AnnotationKind::Widget(widget) = &annotation.kind else {
                return Ok(None);
            };
            if !widget.is_listbox() {
                return Ok(None);
            }
            (
                widget.selected_option_indices(),
                widget.is_multi_select(),
                widget.is_read_only(),
                widget.options.as_ref().map_or(0, Vec::len),
            )
        };
        if option_count == 0 {
            return Err(WidgetEditError::MissingChoiceOptions { id: id.get() });
        }
        if option_index >= option_count {
            return Err(WidgetEditError::ChoiceIndexOutOfBounds {
                id: id.get(),
                index: option_index,
                option_count,
            });
        }
        if read_only {
            return Ok(Some(WidgetActivation {
                state_changed: false,
            }));
        }

        selected.retain(|index| *index < option_count);
        let original_selection = selected.clone();
        if multi_select {
            if let Some(position) = selected.iter().position(|index| *index == option_index) {
                selected.remove(position);
            } else {
                selected.push(option_index);
            }
        } else {
            selected.clear();
            selected.push(option_index);
        }
        selected.sort_unstable();
        selected.dedup();
        if selected == original_selection {
            return Ok(Some(WidgetActivation {
                state_changed: false,
            }));
        }
        self.set_listbox_selection(page_index, id, &selected)?;
        Ok(Some(WidgetActivation {
            state_changed: true,
        }))
    }

    /// Sets selected listbox option indices, overriding read-only for explicit edits.
    pub fn set_listbox_selection(
        &mut self,
        page_index: usize,
        id: AnnotationId,
        selected_indices: &[usize],
    ) -> Result<(), WidgetEditError> {
        let target = WidgetLocation {
            page_index,
            annotation_id: id,
        };
        let (field_id, multi_select, option_values) = {
            let annotation = self.annotation(page_index, id)?;
            let widget = ensure_listbox(annotation, id)?;
            let options = widget
                .options
                .as_ref()
                .ok_or_else(|| WidgetEditError::MissingChoiceOptions { id: id.get() })?;
            (
                widget.field_id,
                widget.is_multi_select(),
                options
                    .iter()
                    .map(|option| option.export_value.clone())
                    .collect::<Vec<_>>(),
            )
        };

        let mut selected_indices = selected_indices.to_vec();
        selected_indices.sort_unstable();
        selected_indices.dedup();
        if !multi_select && selected_indices.len() > 1 {
            return Err(WidgetEditError::MultipleSelectionNotAllowed { id: id.get() });
        }
        if let Some(index) = selected_indices
            .iter()
            .copied()
            .find(|index| *index >= option_values.len())
        {
            return Err(WidgetEditError::ChoiceIndexOutOfBounds {
                id: id.get(),
                index,
                option_count: option_values.len(),
            });
        }

        let selected_values = selected_indices
            .iter()
            .filter_map(|index| option_values.get(*index).cloned())
            .collect::<Vec<_>>();
        for location in self.choice_members(target, field_id) {
            let annotation = self.annotation_mut(location)?;
            let AnnotationKind::Widget(widget) = &mut annotation.kind else {
                continue;
            };
            widget.selected_indices = Some(selected_indices.clone());
            widget.value = choice_value(&selected_values, multi_select);
        }
        Ok(())
    }

    fn set_button_selected(
        &mut self,
        page_index: usize,
        id: AnnotationId,
        selected: bool,
        is_expected_kind: fn(&WidgetAnnotation) -> bool,
        expected_kind: &'static str,
    ) -> Result<(), WidgetEditError> {
        let target = WidgetLocation {
            page_index,
            annotation_id: id,
        };
        let (field_id, is_radio_button, radios_in_unison, target_on_state, target_active) = {
            let annotation = self.annotation(page_index, id)?;
            let widget = ensure_button(annotation, id)?;
            if widget.is_push_button() || !is_expected_kind(widget) {
                return Err(WidgetEditError::WrongButtonKind {
                    id: id.get(),
                    expected: expected_kind,
                });
            }
            let on_state = selected.then(|| annotation.button_on_state()).transpose()?;
            (
                widget.field_id,
                widget.is_radio_button(),
                widget.is_radios_in_unison(),
                on_state,
                widget.active_button_state(annotation).map(Vec::from),
            )
        };

        let members = self.button_members(target, field_id, is_expected_kind);
        if !selected && is_radio_button && target_active.is_none() {
            return Ok(());
        }

        let selected_state = target_on_state.as_deref();
        let group_value = if selected {
            selected_state.map(Vec::from)
        } else if is_radio_button {
            members.iter().find_map(|member| {
                let affected = member.location == target
                    || (radios_in_unison
                        && target_active
                            .as_deref()
                            .is_some_and(|state| member.active_state.as_deref() == Some(state)));
                (!affected).then(|| member.active_state.clone()).flatten()
            })
        } else {
            None
        };

        for member in members {
            let appearance_state = if selected {
                if !is_radio_button || radios_in_unison {
                    (member.on_state.as_deref() == selected_state)
                        .then_some(selected_state)
                        .flatten()
                } else {
                    (member.location == target)
                        .then_some(selected_state)
                        .flatten()
                }
            } else if is_radio_button {
                let affected = member.location == target
                    || (radios_in_unison
                        && target_active
                            .as_deref()
                            .is_some_and(|state| member.active_state.as_deref() == Some(state)));
                (!affected)
                    .then_some(member.active_state.as_deref())
                    .flatten()
            } else {
                None
            };

            let annotation = self.annotation_mut(member.location)?;
            annotation.set_button_appearance_state(appearance_state);
            annotation.set_button_value(group_value.as_deref());
        }
        Ok(())
    }

    fn annotation(
        &self,
        page_index: usize,
        id: AnnotationId,
    ) -> Result<&Annotation, WidgetEditError> {
        let page = self
            .document
            .pages
            .get(page_index)
            .ok_or(WidgetEditError::PageNotFound { page_index })?;
        page.annotation(id)
            .ok_or_else(|| WidgetEditError::AnnotationNotFound {
                page_index,
                id: id.get(),
            })
    }

    fn annotation_mut(
        &mut self,
        location: WidgetLocation,
    ) -> Result<&mut Annotation, WidgetEditError> {
        let page = self.document.pages.get_mut(location.page_index).ok_or(
            WidgetEditError::PageNotFound {
                page_index: location.page_index,
            },
        )?;
        page.annotation_mut(location.annotation_id).ok_or_else(|| {
            WidgetEditError::AnnotationNotFound {
                page_index: location.page_index,
                id: location.annotation_id.get(),
            }
        })
    }

    fn button_members(
        &self,
        target: WidgetLocation,
        field_id: Option<usize>,
        is_expected_kind: fn(&WidgetAnnotation) -> bool,
    ) -> Vec<ButtonMember> {
        let Some(field_id) = field_id else {
            return self
                .annotation(target.page_index, target.annotation_id)
                .ok()
                .map(|annotation| button_member(target, annotation))
                .into_iter()
                .collect();
        };

        self.document
            .pages
            .iter()
            .enumerate()
            .flat_map(|(page_index, page)| {
                page.annotations
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(move |annotation| {
                        let AnnotationKind::Widget(widget) = &annotation.kind else {
                            return None;
                        };
                        (widget.field_id == Some(field_id)
                            && widget.is_button()
                            && !widget.is_push_button()
                            && is_expected_kind(widget))
                        .then(|| {
                            button_member(
                                WidgetLocation {
                                    page_index,
                                    annotation_id: annotation.id(),
                                },
                                annotation,
                            )
                        })
                    })
            })
            .collect()
    }

    fn choice_members(
        &self,
        target: WidgetLocation,
        field_id: Option<usize>,
    ) -> Vec<WidgetLocation> {
        let Some(field_id) = field_id else {
            return vec![target];
        };

        self.document
            .pages
            .iter()
            .enumerate()
            .flat_map(|(page_index, page)| {
                page.annotations
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(move |annotation| {
                        let AnnotationKind::Widget(widget) = &annotation.kind else {
                            return None;
                        };
                        (widget.field_id == Some(field_id) && widget.is_listbox()).then_some(
                            WidgetLocation {
                                page_index,
                                annotation_id: annotation.id(),
                            },
                        )
                    })
            })
            .collect()
    }
}

fn choice_value(values: &[Vec<u8>], multi_select: bool) -> Option<WidgetFieldValue> {
    if values.is_empty() {
        None
    } else if multi_select {
        Some(WidgetFieldValue::Array(
            values
                .iter()
                .cloned()
                .map(WidgetFieldValue::Bytes)
                .collect(),
        ))
    } else {
        values.first().cloned().map(WidgetFieldValue::Bytes)
    }
}

fn button_member(location: WidgetLocation, annotation: &Annotation) -> ButtonMember {
    let active_state = match &annotation.kind {
        AnnotationKind::Widget(widget) => widget.active_button_state(annotation).map(Vec::from),
        _ => None,
    };
    ButtonMember {
        location,
        on_state: annotation.button_on_state().ok(),
        active_state,
    }
}

fn ensure_listbox(
    annotation: &Annotation,
    id: AnnotationId,
) -> Result<&WidgetAnnotation, WidgetEditError> {
    let AnnotationKind::Widget(widget) = &annotation.kind else {
        return Err(WidgetEditError::WrongSubtype {
            id: id.get(),
            subtype: String::from_utf8_lossy(&annotation.subtype).into_owned(),
        });
    };
    widget
        .is_listbox()
        .then_some(widget)
        .ok_or_else(|| WidgetEditError::NotListbox { id: id.get() })
}

fn ensure_button(
    annotation: &Annotation,
    id: AnnotationId,
) -> Result<&WidgetAnnotation, WidgetEditError> {
    let AnnotationKind::Widget(widget) = &annotation.kind else {
        return Err(WidgetEditError::WrongSubtype {
            id: id.get(),
            subtype: String::from_utf8_lossy(&annotation.subtype).into_owned(),
        });
    };
    widget
        .is_button()
        .then_some(widget)
        .ok_or_else(|| WidgetEditError::NotButton { id: id.get() })
}
