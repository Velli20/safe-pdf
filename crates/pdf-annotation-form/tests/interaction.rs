#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::as_conversions
)]

use std::time::{Duration, Instant};

use pdf_annotation_form::{
    AnnotationController, AnnotationEditCommand, AnnotationInteractionError,
    AnnotationInteractionResult, AnnotationPointerMove, AnnotationPointerPress, AnnotationViewport,
    FreeText, FreeTextEditError, FreeTextEditor, FreeTextStyle,
};
use pdf_annotation_types::{AnnotationKind, annotation_id::AnnotationId};
use pdf_document::{document::PdfDocument, page::PdfPage};
use pdf_graphics::{point::Point, rect::Rect};

trait TestAnnotationController {
    fn test_pointer_pressed(
        &mut self,
        page_index: usize,
        document: &mut PdfDocument,
        viewport: AnnotationViewport,
        position: Point,
        timestamp: Instant,
    ) -> Result<AnnotationInteractionResult, AnnotationInteractionError>;

    fn test_pointer_moved(
        &mut self,
        page_index: usize,
        document: &mut PdfDocument,
        viewport: AnnotationViewport,
        position: Point,
    ) -> Result<AnnotationInteractionResult, AnnotationInteractionError>;
}

impl TestAnnotationController for AnnotationController {
    fn test_pointer_pressed(
        &mut self,
        page_index: usize,
        document: &mut PdfDocument,
        viewport: AnnotationViewport,
        position: Point,
        timestamp: Instant,
    ) -> Result<AnnotationInteractionResult, AnnotationInteractionError> {
        self.pointer_pressed(
            document,
            AnnotationPointerPress {
                page_index,
                viewport,
                position,
                timestamp,
            },
        )
    }

    fn test_pointer_moved(
        &mut self,
        page_index: usize,
        document: &mut PdfDocument,
        viewport: AnnotationViewport,
        position: Point,
    ) -> Result<AnnotationInteractionResult, AnnotationInteractionError> {
        self.pointer_moved(
            document,
            AnnotationPointerMove {
                page_index,
                viewport,
                position,
            },
        )
    }
}

fn document_with_free_text(rect: Rect) -> (PdfDocument, AnnotationId) {
    let mut page = PdfPage {
        media_box: Some(Rect {
            left: 0.0,
            top: 0.0,
            right: 200.0,
            bottom: 100.0,
        }),
        ..Default::default()
    };
    let id = FreeTextEditor::new(&mut page)
        .create(FreeText {
            rect,
            text: "drag me".to_owned(),
            style: FreeTextStyle::default(),
        })
        .expect("generated FreeText should be valid");
    (PdfDocument { pages: vec![page] }, id)
}

fn viewport(document: &PdfDocument) -> AnnotationViewport {
    let page = document.pages.first().expect("page should exist");
    AnnotationViewport::from_page(page, 400.0, 200.0).expect("viewport should be valid")
}

fn annotation_rect(document: &PdfDocument, id: AnnotationId) -> Rect {
    document
        .pages
        .first()
        .expect("page should exist")
        .annotation(id)
        .and_then(|annotation| annotation.rect)
        .expect("annotation should have a rectangle")
        .normalized()
}

fn begin_editing(
    controller: &mut AnnotationController,
    document: &mut PdfDocument,
    viewport: AnnotationViewport,
) {
    let first_click = Instant::now();
    controller
        .test_pointer_pressed(0, document, viewport, Point::new(80.0, 140.0), first_click)
        .expect("first click should select FreeText");
    controller.pointer_released();
    controller
        .test_pointer_pressed(
            0,
            document,
            viewport,
            Point::new(80.0, 140.0),
            first_click
                .checked_add(Duration::from_millis(100))
                .expect("second click time should be representable"),
        )
        .expect("second click should begin editing");
    assert!(controller.is_editing());
}

fn handle_command(
    controller: &mut AnnotationController,
    document: &mut PdfDocument,
    command: AnnotationEditCommand<'_>,
) -> Result<AnnotationInteractionResult, AnnotationInteractionError> {
    let page = document.pages.first_mut().expect("page should exist");
    controller.handle_edit_command(page, command)
}

fn annotation_text(document: &mut PdfDocument, id: AnnotationId) -> String {
    let page = document.pages.first_mut().expect("page should exist");
    FreeTextEditor::new(page)
        .get(id)
        .expect("FreeText should remain editable")
        .text
}

#[test]
fn edit_commands_update_unicode_text_by_character_position() {
    let (mut document, id) = document_with_free_text(Rect {
        left: 20.0,
        top: 20.0,
        right: 160.0,
        bottom: 60.0,
    });
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();
    begin_editing(&mut controller, &mut document, viewport);

    handle_command(
        &mut controller,
        &mut document,
        AnnotationEditCommand::MoveToStart,
    )
    .expect("caret should move to start");
    handle_command(
        &mut controller,
        &mut document,
        AnnotationEditCommand::Insert { text: "é" },
    )
    .expect("WinAnsi text should insert");
    handle_command(
        &mut controller,
        &mut document,
        AnnotationEditCommand::MoveRight,
    )
    .expect("caret should move right");
    handle_command(
        &mut controller,
        &mut document,
        AnnotationEditCommand::DeleteBackward,
    )
    .expect("backward delete should remove one character");
    handle_command(
        &mut controller,
        &mut document,
        AnnotationEditCommand::DeleteForward,
    )
    .expect("forward delete should remove one character");
    handle_command(
        &mut controller,
        &mut document,
        AnnotationEditCommand::Newline,
    )
    .expect("newline should insert");
    handle_command(
        &mut controller,
        &mut document,
        AnnotationEditCommand::Commit,
    )
    .expect("edit should commit");

    assert_eq!(annotation_text(&mut document, id), "é\nag me");
    assert!(!controller.is_editing());
}

#[test]
fn cancel_restores_original_text() {
    let (mut document, id) = document_with_free_text(Rect {
        left: 20.0,
        top: 20.0,
        right: 160.0,
        bottom: 60.0,
    });
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();
    begin_editing(&mut controller, &mut document, viewport);
    handle_command(
        &mut controller,
        &mut document,
        AnnotationEditCommand::Insert { text: "!" },
    )
    .expect("text should insert");
    assert_eq!(annotation_text(&mut document, id), "drag me!");

    handle_command(
        &mut controller,
        &mut document,
        AnnotationEditCommand::Cancel,
    )
    .expect("edit should cancel");

    assert_eq!(annotation_text(&mut document, id), "drag me");
    assert!(!controller.is_editing());
}

#[test]
fn rejected_edit_keeps_document_and_session_unchanged() {
    let (mut document, id) = document_with_free_text(Rect {
        left: 20.0,
        top: 20.0,
        right: 160.0,
        bottom: 60.0,
    });
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();
    begin_editing(&mut controller, &mut document, viewport);

    let error = handle_command(
        &mut controller,
        &mut document,
        AnnotationEditCommand::Insert { text: "😀" },
    )
    .expect_err("unsupported text should be rejected");
    assert!(matches!(
        error,
        AnnotationInteractionError::FreeText(FreeTextEditError::FontError(
            pdf_font::error::FontError::UnsupportedWinAnsiCharacter { character: '😀' }
        ))
    ));
    assert_eq!(annotation_text(&mut document, id), "drag me");
    assert!(controller.is_editing());

    handle_command(
        &mut controller,
        &mut document,
        AnnotationEditCommand::Insert { text: "!" },
    )
    .expect("a subsequent valid edit should succeed");
    assert_eq!(annotation_text(&mut document, id), "drag me!");
}

#[test]
fn clicking_away_commits_and_closes_the_edit_session() {
    let (mut document, id) = document_with_free_text(Rect {
        left: 20.0,
        top: 20.0,
        right: 160.0,
        bottom: 60.0,
    });
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();
    begin_editing(&mut controller, &mut document, viewport);
    handle_command(
        &mut controller,
        &mut document,
        AnnotationEditCommand::Insert { text: "!" },
    )
    .expect("text should insert");

    let outcome = controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(390.0, 190.0),
            Instant::now(),
        )
        .expect("background click should succeed");

    assert_eq!(outcome, AnnotationInteractionResult::REDRAW);
    assert_eq!(controller.selected(), None);
    assert!(!controller.is_editing());
    assert_eq!(annotation_text(&mut document, id), "drag me!");
}

#[test]
fn page_change_clears_an_edit_session() {
    let (mut document, _) = document_with_free_text(Rect {
        left: 20.0,
        top: 20.0,
        right: 160.0,
        bottom: 60.0,
    });
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();
    begin_editing(&mut controller, &mut document, viewport);

    let outcome = controller.page_changed();

    assert_eq!(outcome, AnnotationInteractionResult::REDRAW);
    assert_eq!(controller.selected(), None);
    assert!(!controller.is_editing());
}

#[test]
fn edit_command_is_ignored_without_an_active_session() {
    let (mut document, _) = document_with_free_text(Rect {
        left: 20.0,
        top: 20.0,
        right: 160.0,
        bottom: 60.0,
    });
    let outcome = handle_command(
        &mut AnnotationController::default(),
        &mut document,
        AnnotationEditCommand::MoveToEnd,
    )
    .expect("inactive command should be ignored");

    assert_eq!(outcome, AnnotationInteractionResult::IGNORED);
}

#[test]
fn drag_moves_free_text_in_pdf_space_and_preserves_size() {
    let requested = Rect {
        left: 20.0,
        top: 20.0,
        right: 60.0,
        bottom: 40.0,
    };
    let (mut document, id) = document_with_free_text(requested);
    let original = annotation_rect(&document, id);
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();

    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(80.0, 140.0),
            Instant::now(),
        )
        .expect("press should select FreeText");
    let outcome = controller
        .test_pointer_moved(0, &mut document, viewport, Point::new(100.0, 130.0))
        .expect("drag should move FreeText");

    assert_eq!(
        annotation_rect(&document, id),
        Rect {
            left: original.left + 10.0,
            top: original.top + 5.0,
            right: original.right + 10.0,
            bottom: original.bottom + 5.0,
        }
    );
    assert!(outcome.consumed);
    assert!(outcome.redraw);
    assert_eq!(controller.selected(), Some(id));
}

#[test]
fn movement_below_drag_threshold_leaves_rect_unchanged() {
    let requested = Rect {
        left: 20.0,
        top: 20.0,
        right: 60.0,
        bottom: 40.0,
    };
    let (mut document, id) = document_with_free_text(requested);
    let original = annotation_rect(&document, id);
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();

    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(80.0, 140.0),
            Instant::now(),
        )
        .expect("press should select FreeText");
    let outcome = controller
        .test_pointer_moved(0, &mut document, viewport, Point::new(82.0, 142.0))
        .expect("small movement should be accepted");

    assert_eq!(annotation_rect(&document, id), original);
    assert!(outcome.consumed);
    assert!(!outcome.redraw);
    assert!(controller.pointer_released().consumed);
    assert!(!controller.pointer_released().consumed);
}

#[test]
fn drag_clamps_free_text_to_media_box() {
    let requested = Rect {
        left: 150.0,
        top: 60.0,
        right: 180.0,
        bottom: 90.0,
    };
    let (mut document, id) = document_with_free_text(requested);
    let original = annotation_rect(&document, id);
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();

    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(330.0, 50.0),
            Instant::now(),
        )
        .expect("press should select FreeText");
    controller
        .test_pointer_moved(0, &mut document, viewport, Point::new(530.0, -150.0))
        .expect("drag should clamp FreeText");

    assert_eq!(
        annotation_rect(&document, id),
        Rect {
            left: 200.0 - original.width(),
            top: 100.0 - original.height(),
            right: 200.0,
            bottom: 100.0,
        }
    );
}

#[test]
fn annotation_flags_do_not_disable_explicit_editor_interaction() {
    const EXISTING_ANNOTATION_FLAGS: i32 = 708;
    let (mut document, id) = document_with_free_text(Rect {
        left: 20.0,
        top: 20.0,
        right: 60.0,
        bottom: 40.0,
    });
    let original = annotation_rect(&document, id);
    document
        .pages
        .first_mut()
        .expect("page should exist")
        .annotation_mut(id)
        .expect("annotation should exist")
        .flags = Some(EXISTING_ANNOTATION_FLAGS);
    let drag_viewport = viewport(&document);
    let mut controller = AnnotationController::default();
    let first_click = Instant::now();

    controller
        .test_pointer_pressed(
            0,
            &mut document,
            drag_viewport,
            Point::new(80.0, 140.0),
            first_click,
        )
        .expect("first click should select FreeText");
    controller
        .test_pointer_moved(0, &mut document, drag_viewport, Point::new(100.0, 130.0))
        .expect("annotation flags should not suppress dragging");
    assert_ne!(annotation_rect(&document, id), original);

    let (mut document, id) = document_with_free_text(Rect {
        left: 20.0,
        top: 20.0,
        right: 60.0,
        bottom: 40.0,
    });
    document
        .pages
        .first_mut()
        .expect("page should exist")
        .annotation_mut(id)
        .expect("annotation should exist")
        .flags = Some(EXISTING_ANNOTATION_FLAGS);
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();
    let first_click = Instant::now();
    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(80.0, 140.0),
            first_click,
        )
        .expect("first click should select FreeText");
    controller.pointer_released();
    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(80.0, 140.0),
            first_click
                .checked_add(Duration::from_millis(100))
                .expect("second click time should be representable"),
        )
        .expect("second click should begin editing");

    assert!(controller.is_editing());
}

#[test]
fn existing_free_text_variant_drags_without_editor_decode() {
    let (mut document, id) = document_with_free_text(Rect {
        left: 20.0,
        top: 20.0,
        right: 60.0,
        bottom: 40.0,
    });
    let original = annotation_rect(&document, id);
    let annotation = document
        .pages
        .first_mut()
        .expect("page should exist")
        .annotation_mut(id)
        .expect("annotation should exist");
    assert!(matches!(annotation.kind, AnnotationKind::FreeText(_)));
    if let AnnotationKind::FreeText(free_text) = &mut annotation.kind {
        free_text.rich_contents = Some(b"<p>rich</p>".to_vec());
    }
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();
    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(80.0, 140.0),
            Instant::now(),
        )
        .expect("existing FreeText should select");
    let moved = controller
        .test_pointer_moved(0, &mut document, viewport, Point::new(100.0, 130.0))
        .expect("existing FreeText should move by its rectangle");

    assert_eq!(controller.selected(), Some(id));
    assert!(!controller.is_editing());
    assert_ne!(annotation_rect(&document, id), original);
    assert!(moved.redraw);
}

#[test]
fn page_change_cancels_pending_drag() {
    let (mut document, id) = document_with_free_text(Rect {
        left: 20.0,
        top: 20.0,
        right: 60.0,
        bottom: 40.0,
    });
    let original = annotation_rect(&document, id);
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();

    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(80.0, 140.0),
            Instant::now(),
        )
        .expect("press should select FreeText");
    controller.page_changed();
    let moved = controller
        .test_pointer_moved(0, &mut document, viewport, Point::new(100.0, 130.0))
        .expect("movement after page change should be ignored");

    assert_eq!(annotation_rect(&document, id), original);
    assert_eq!(moved, AnnotationInteractionResult::default());
}

#[test]
fn removed_annotation_returns_generic_interaction_error() {
    let (mut document, id) = document_with_free_text(Rect {
        left: 20.0,
        top: 20.0,
        right: 60.0,
        bottom: 40.0,
    });
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();
    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(80.0, 140.0),
            Instant::now(),
        )
        .expect("press should select FreeText");
    document
        .pages
        .first_mut()
        .expect("page should exist")
        .take_annotation(id)
        .expect("annotation should be removable");

    let error = controller
        .test_pointer_moved(0, &mut document, viewport, Point::new(100.0, 130.0))
        .expect_err("a removed drag target should return an error");

    assert!(matches!(
        error,
        AnnotationInteractionError::AnnotationNotFound { id: missing } if missing == id.get()
    ));
}

#[test]
fn non_finite_drag_position_does_not_change_annotation() {
    let (mut document, id) = document_with_free_text(Rect {
        left: 20.0,
        top: 20.0,
        right: 60.0,
        bottom: 40.0,
    });
    let original = annotation_rect(&document, id);
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();
    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(80.0, 140.0),
            Instant::now(),
        )
        .expect("press should select FreeText");

    let outcome = controller
        .test_pointer_moved(0, &mut document, viewport, Point::new(f32::NAN, 130.0))
        .expect("invalid movement should be ignored");

    assert_eq!(annotation_rect(&document, id), original);
    assert!(outcome.consumed);
    assert!(!outcome.redraw);
}

#[test]
fn text_edit_mode_disables_dragging() {
    let requested = Rect {
        left: 20.0,
        top: 20.0,
        right: 60.0,
        bottom: 40.0,
    };
    let (mut document, id) = document_with_free_text(requested);
    let original = annotation_rect(&document, id);
    let viewport = viewport(&document);
    let mut controller = AnnotationController::default();
    let first_click = Instant::now();

    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(80.0, 140.0),
            first_click,
        )
        .expect("first click should select FreeText");
    controller.pointer_released();
    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(80.0, 140.0),
            first_click
                .checked_add(Duration::from_millis(100))
                .expect("second click time should be representable"),
        )
        .expect("second click should begin editing");
    let outcome = controller
        .test_pointer_moved(0, &mut document, viewport, Point::new(100.0, 130.0))
        .expect("movement while editing should be ignored");

    assert!(controller.is_editing());
    assert_eq!(annotation_rect(&document, id), original);
    assert_eq!(outcome, AnnotationInteractionResult::default());
}
