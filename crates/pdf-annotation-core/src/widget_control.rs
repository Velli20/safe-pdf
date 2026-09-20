//! Derives host control models from resolved widget fields.
use crate::entry::{ControlKind, ControlTag, WidgetControl};
use crate::{
    fields::{ChoiceOption, ComboValue, Field, FieldKind, ListSelection, OptionId, TextMode},
    kind::AnnotationKind,
    models::Annotation,
    pdf_data::NativeAnnotation,
    widgets::WidgetBinding,
};
use num_traits::ToPrimitive;

/// Builds the control for a widget bound to `field`, or `None` for unsupported fields.
pub(crate) fn control(annotation: &Annotation, field: &Field) -> Option<WidgetControl> {
    let AnnotationKind::Widget(widget) = &annotation.content else {
        return None;
    };
    let option = match widget.binding {
        WidgetBinding::Field { .. } => None,
        WidgetBinding::RadioOption { option, .. }
        | WidgetBinding::CheckboxOption { option, .. } => Some(option),
    };
    let source = annotation
        .metadata
        .source
        .as_deref()
        .and_then(|source| match &source.kind {
            NativeAnnotation::Widget(widget) => Some(widget.as_ref()),
            _ => None,
        });
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
        checked: false,
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
        FieldKind::Checkbox(checkbox) => {
            model.kind = ControlKind::Checkbox;
            model.input_type = Some("checkbox".into());
            model.checked = checkbox.checked;
        }
        FieldKind::CheckboxGroup(group) => {
            model.kind = ControlKind::CheckboxGroup;
            model.input_type = Some("checkbox".into());
            model.checked = option.is_some() && group.selected == option;
        }
        FieldKind::Radio(radio) => {
            model.kind = ControlKind::Radio;
            model.input_type = Some("radio".into());
            model.checked = option.is_some() && radio.selected == option;
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

/// Positions of the options whose identities appear in `ids`.
fn indices(options: &[ChoiceOption], ids: &[OptionId]) -> Vec<u32> {
    options
        .iter()
        .enumerate()
        .filter(|(_, option)| ids.contains(&option.id))
        .filter_map(|(index, _)| index.to_u32())
        .collect()
}
