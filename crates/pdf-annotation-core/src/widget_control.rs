//! A widget's bound field and the host control and button state derived from it.
use crate::entry::{ControlKind, ControlTag, WidgetControl};
use crate::{
    engine::DocumentView,
    fields::{ChoiceOption, ComboValue, Field, FieldKind, ListSelection, OptionId, TextMode},
    kind::AnnotationKind,
    layer_error::{AnnotationLayerError, AnnotationLayerResult},
    models::Annotation,
};
use num_traits::ToPrimitive;

/// The appearance state a widget selects when it is painted on the page canvas.
pub(crate) enum PaintedState {
    /// A push button or read-only text field draws its source `/AS`.
    Source,
    /// A checkbox or radio option draws its on-state when selected, else `/Off`.
    Checked(bool),
}

impl Annotation {
    /// The shared field this widget is bound to; `None` for other kinds.
    ///
    /// # Errors
    /// Returns an error when the bound field is missing from `view`.
    pub(crate) fn widget_field<'a>(
        &self,
        view: &DocumentView<'a>,
    ) -> AnnotationLayerResult<Option<&'a Field>> {
        self.content
            .bound_field()
            .map(|id| {
                view.field(id)
                    .ok_or(AnnotationLayerError::InvalidInput("widget field"))
            })
            .transpose()
    }

    /// Builds the control for a widget bound to `field`, or `None` for unsupported fields.
    pub(crate) fn widget_control(&self, field: &Field) -> Option<WidgetControl> {
        let AnnotationKind::Widget(widget) = &self.content else {
            return None;
        };
        let option = widget.binding.option();
        let painted_state = self.painted_state(field);
        let source = self.source_widget();
        let definition = &field.definition;
        let mut model = WidgetControl {
            tag: ControlTag::Input,
            input_type: None,
            kind: ControlKind::Text,
            field: widget.binding.field(),
            option,
            label: definition.label.clone(),
            required: definition.required,
            value: String::new(),
            max_length: None,
            checked: matches!(painted_state, Some(PaintedState::Checked(true))),
            caption: String::new(),
            options: None,
            multiple: false,
            list_box: false,
            selected: Vec::new(),
            top_index: 0,
            rotation: source
                .and_then(|w| w.appearance_characteristics.as_ref())
                .and_then(|mk| mk.rotation)
                .unwrap_or(0),
            painted: painted_state.is_some() && self.has_normal_appearance(),
        };
        match &definition.content {
            FieldKind::Text(text) => {
                model.tag = if text.mode == TextMode::Multiline {
                    ControlTag::TextArea
                } else {
                    ControlTag::Input
                };
                model.input_type = Some(
                    if text.mode == TextMode::Password {
                        "password"
                    } else {
                        "text"
                    }
                    .into(),
                );
                model.value = text.value.clone();
                model.max_length = text.max_length;
            }
            FieldKind::Checkbox(_) => {
                model.kind = ControlKind::Checkbox;
                model.input_type = Some("checkbox".into());
            }
            FieldKind::CheckboxGroup(_) => {
                model.kind = ControlKind::CheckboxGroup;
                model.input_type = Some("checkbox".into());
            }
            FieldKind::Radio(_) => {
                model.kind = ControlKind::Radio;
                model.input_type = Some("radio".into());
            }
            FieldKind::Button(button) => {
                model.kind = ControlKind::Button;
                model.tag = ControlTag::Button;
                model.caption = button.caption.clone();
            }
            FieldKind::ListBox(list) => {
                let ids: Vec<OptionId> = match &list.selection {
                    ListSelection::Multiple(ids) => ids.clone(),
                    ListSelection::Single(id) => id.iter().copied().collect(),
                };
                model.kind = ControlKind::ListBox;
                model.tag = ControlTag::Select;
                model.multiple = matches!(list.selection, ListSelection::Multiple(_));
                model.list_box = true;
                model.selected = indices(&list.options, &ids);
                model.top_index = source
                    .and_then(|w| w.top_index)
                    .and_then(|index| index.to_u32())
                    .unwrap_or(0);
                model.options = Some(list.options.clone());
            }
            FieldKind::ComboBox(combo) => {
                model.kind = ControlKind::ComboBox;
                model.tag = if combo.editable {
                    ControlTag::Input
                } else {
                    ControlTag::Select
                };
                model.input_type = Some("text".into());
                match &combo.value {
                    ComboValue::Option(id) => {
                        model.selected = indices(&combo.options, std::slice::from_ref(id));
                        model.value = combo
                            .options
                            .iter()
                            .find(|o| o.id == *id)
                            .map(|o| o.export_value.clone())
                            .unwrap_or_default();
                    }
                    ComboValue::Text(text) => model.value = text.clone(),
                    ComboValue::Empty => {}
                }
                model.options = Some(combo.options.clone());
            }
            _ => return None,
        }
        Some(model)
    }

    /// How this widget, bound to `field`, selects its appearance state; `None` for
    /// non-widgets and fields whose control presents their value instead: editable text
    /// fields and choice fields.
    pub(crate) fn painted_state(&self, field: &Field) -> Option<PaintedState> {
        let AnnotationKind::Widget(widget) = &self.content else {
            return None;
        };
        let option = widget.binding.option();
        let selects = |selected: Option<OptionId>| option.is_some() && selected == option;
        Some(match &field.definition.content {
            FieldKind::Button(_) => PaintedState::Source,
            // A read-only value never changes, so its source appearance stays current.
            FieldKind::Text(_) if field.definition.read_only => PaintedState::Source,
            FieldKind::Checkbox(checkbox) => PaintedState::Checked(checkbox.checked),
            FieldKind::CheckboxGroup(group) => PaintedState::Checked(selects(group.selected)),
            FieldKind::Radio(radio) => PaintedState::Checked(selects(radio.selected)),
            _ => return None,
        })
    }
}

/// Positions of the options whose identities appear in `ids`.
fn indices(options: &[ChoiceOption], ids: &[OptionId]) -> Vec<u32> {
    options
        .iter()
        .enumerate()
        .filter(|(_, option)| ids.contains(&option.id))
        .filter_map(|(index, _)| index.to_u32())
        .collect()
}
