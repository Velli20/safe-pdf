//! Triggers that run annotation actions, and the selection of what each one runs.
use serde::{Deserialize, Serialize};

use crate::{
    models::Annotation,
    pdf_data::{
        AnnotationAction, HideTarget, NativeAnnotation, WidgetAdditionalActions, WidgetAnnotation,
    },
};
use pdf_object_reader::text_string;

/// A user interaction that runs an annotation's action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(rename_all = "snake_case")]
pub enum ActionTrigger {
    /// The annotation is activated, such as a click releasing over it: its `/A` action.
    Activate,
    /// The pointer enters the annotation.
    CursorEnter,
    /// The pointer leaves the annotation.
    CursorExit,
    /// A pointer button is pressed inside the annotation.
    MouseDown,
    /// A pointer button is released inside the annotation.
    MouseUp,
    /// The annotation receives input focus.
    Focus,
    /// The annotation loses input focus.
    Blur,
}

impl ActionTrigger {
    /// Every trigger, in declaration order.
    const ALL: [Self; 7] = [
        Self::Activate,
        Self::CursorEnter,
        Self::CursorExit,
        Self::MouseDown,
        Self::MouseUp,
        Self::Focus,
        Self::Blur,
    ];

    /// The `/AA` entry this trigger runs; `Activate` runs `/A` instead.
    fn additional_action(self, actions: &WidgetAdditionalActions) -> Option<&AnnotationAction> {
        match self {
            Self::Activate => None,
            Self::CursorEnter => actions.cursor_enter.as_ref(),
            Self::CursorExit => actions.cursor_exit.as_ref(),
            Self::MouseDown => actions.mouse_down.as_ref(),
            Self::MouseUp => actions.mouse_up.as_ref(),
            Self::Focus => actions.focus.as_ref(),
            Self::Blur => actions.blur.as_ref(),
        }
    }
}

impl Annotation {
    /// The action `trigger` runs: the current action on activation, else the retained
    /// widget's `/AA` entry. Returning it never executes it.
    pub fn trigger_action(&self, trigger: ActionTrigger) -> Option<AnnotationAction> {
        match trigger {
            ActionTrigger::Activate => self.action(),
            trigger => self
                .source_widget()
                .and_then(|widget| widget.additional_actions.as_ref())
                .and_then(|actions| trigger.additional_action(actions))
                .cloned(),
        }
    }

    /// The triggers that run an action on this annotation.
    pub fn triggers(&self) -> Vec<ActionTrigger> {
        ActionTrigger::ALL
            .into_iter()
            .filter(|trigger| match trigger {
                ActionTrigger::Activate => self.action().is_some(),
                trigger => self
                    .source_widget()
                    .and_then(|widget| widget.additional_actions.as_ref())
                    .is_some_and(|actions| trigger.additional_action(actions).is_some()),
            })
            .collect()
    }

    /// Whether a hide action naming `target` applies to this annotation.
    pub(crate) fn is_hide_target(&self, target: &HideTarget) -> bool {
        let Some(source) = self.source() else {
            return false;
        };
        let widget = self.source_widget();
        match target {
            HideTarget::Object { number } => {
                source.object_number == Some(*number)
                    || widget.is_some_and(|widget| widget.field_id == Some(*number))
            }
            HideTarget::FieldName(name) => widget.is_some_and(|widget| widget.is_named(name)),
            HideTarget::Unresolved => false,
        }
    }

    /// The retained source widget record, if this annotation was adopted from one.
    fn source_widget(&self) -> Option<&WidgetAnnotation> {
        match &self.source()?.kind {
            NativeAnnotation::Widget(widget) => Some(widget),
            _ => None,
        }
    }
}

impl AnnotationAction {
    /// The actions run in order: a sequence's members, else this action alone.
    pub fn steps(&self) -> &[AnnotationAction] {
        match self {
            Self::Sequence(actions) => actions,
            action => std::slice::from_ref(action),
        }
    }
}

impl WidgetAnnotation {
    /// Whether `name` is this widget's fully qualified field name or that of a field
    /// above it. Names that are not valid text strings match nothing.
    fn is_named(&self, name: &[u8]) -> bool {
        let Ok(name) = text_string::decode(name) else {
            return false;
        };
        let Some(qualified) = self.qualified_name() else {
            return false;
        };
        qualified
            .strip_prefix(name.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    }

    /// The partial names joined by periods, or `None` when there are none or one is
    /// not a valid text string.
    fn qualified_name(&self) -> Option<String> {
        if self.partial_names.is_empty() {
            return None;
        }
        let names = self
            .partial_names
            .iter()
            .map(|name| text_string::decode(name))
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        Some(names.join("."))
    }
}
