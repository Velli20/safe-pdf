#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use pdf_annotation_form::{
    FreeText, FreeTextEditError, FreeTextEditor, FreeTextOverflow, FreeTextStyle,
};
use pdf_document::page::PdfPage;
use pdf_font::{font::Font, standard14::Standard14Font, true_type_font::TrueTypeFont};
use pdf_graphics::rect::Rect;

fn created_rect(free_text: FreeText) -> Rect {
    let mut page = PdfPage::default();
    let id = FreeTextEditor::new(&mut page)
        .create(free_text)
        .expect("FreeText layout should be valid");
    page.annotation(id)
        .and_then(|annotation| annotation.rect)
        .expect("generated FreeText should have a rectangle")
}

#[test]
fn expand_right_grows_to_the_longest_line() {
    let free_text = FreeText {
        rect: Rect::new(10.0, 20.0),
        text: "a long line".to_owned(),
        style: FreeTextStyle::default(),
    };

    let grown = created_rect(free_text.clone());

    assert!(grown.width() > free_text.rect.width());
    assert!(grown.height() >= free_text.rect.height());
}

#[test]
fn explicit_newlines_preserve_empty_lines() {
    let free_text = FreeText {
        rect: Rect::new(100.0, 100.0),
        text: "first\n\nthird".to_owned(),
        style: FreeTextStyle::default(),
    };

    let first_line = free_text.caret_rect(5).expect("first line should exist");
    let empty_line = free_text.caret_rect(6).expect("empty line should exist");
    let third_line = free_text.caret_rect(7).expect("third line should exist");

    assert!(empty_line.top < first_line.top);
    assert!(third_line.top < empty_line.top);
}

#[test]
fn caret_follows_whitespace_wrap_boundaries_and_empty_lines() {
    let font = Font::TrueType(TrueTypeFont::synthetic_standard14_font(
        Standard14Font::Courier,
    ));
    let mut style = FreeTextStyle::default();
    style.font.standard14 = Standard14Font::Courier;
    style.font_size = 12.0;
    style.line_height = 14.0;
    style.insets = Rect {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    style.overflow = FreeTextOverflow::ExpandHeight;
    let free_text = FreeText {
        rect: Rect::new(font.encoded_text_width(b"aaaa", style.font_size), 20.0),
        text: "  aa b cccccc\n".to_owned(),
        style,
    };

    let before_text = free_text.caret_rect(0).expect("cursor should be valid");
    let within_leading_whitespace = free_text.caret_rect(1).expect("cursor should be valid");
    let first_wrap_boundary = free_text.caret_rect(6).expect("cursor should be valid");
    let second_line_start = free_text.caret_rect(7).expect("cursor should be valid");
    let second_wrap_boundary = free_text.caret_rect(11).expect("cursor should be valid");
    let third_line = free_text.caret_rect(12).expect("cursor should be valid");
    let trailing_empty_line = free_text.caret_rect(14).expect("cursor should be valid");

    assert_eq!(before_text.left, within_leading_whitespace.left);
    assert_eq!(before_text.top, first_wrap_boundary.top);
    assert_eq!(second_line_start.top, second_wrap_boundary.top);
    assert!(second_line_start.top < first_wrap_boundary.top);
    assert!(third_line.top < second_wrap_boundary.top);
    assert!(trailing_empty_line.top < third_line.top);
}

#[test]
fn expand_height_grows_and_reject_reports_the_same_overflow() {
    let mut style = FreeTextStyle::default();
    style.font.standard14 = Standard14Font::Courier;
    style.overflow = FreeTextOverflow::ExpandHeight;
    let free_text = FreeText {
        rect: Rect::new(20.0, 16.0),
        text: "one two three four".to_owned(),
        style,
    };

    let grown = created_rect(free_text.clone());
    assert!(grown.height() > free_text.rect.height());

    let mut rejected = free_text;
    rejected.style.overflow = FreeTextOverflow::Reject;
    let error = FreeTextEditor::new(&mut PdfPage::default())
        .create(rejected)
        .expect_err("the same wrapped text should not fit in Reject mode");
    assert!(matches!(
        error,
        FreeTextEditError::InvalidInput {
            field: "rectangle",
            reason: "text does not fit within the requested height",
        }
    ));
}
