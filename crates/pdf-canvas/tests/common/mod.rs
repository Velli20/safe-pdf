#![allow(clippy::arithmetic_side_effects, clippy::expect_used)]

use std::sync::Arc;

use pdf_canvas::{
    canvas_backend::{CanvasBackend, Image, Shader},
    error::PdfCanvasError,
    recording_canvas::RecordingCanvas,
    stroke_style::StrokeStyle,
};
use pdf_content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_graphics::{
    BlendMode, MaskMode, PathFillType, PixelFormat, color::Color, pdf_path::PdfPath, rect::Rect,
    transform::Transform,
};
use pdf_object::{
    dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    stream::StreamObject,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ObservedImage {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub pixel_format: PixelFormat,
    pub blend_mode: Option<BlendMode>,
    pub dest_rect: Rect,
    pub image_rotation: Option<f32>,
}

#[derive(Default)]
pub struct ObservingCanvas {
    pub fill_colors: Vec<Color>,
    pub stroke_styles: Vec<StrokeStyle>,
    pub images: Vec<ObservedImage>,
    pub inline_images: Vec<ObservedImage>,
    pub save_count: usize,
    pub restore_count: usize,
    pub begin_mask_count: usize,
}

impl ObservingCanvas {
    fn observed_image(
        image: &Image,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    ) -> ObservedImage {
        ObservedImage {
            data: image.data.to_vec(),
            width: image.width,
            height: image.height,
            pixel_format: image.pixel_format,
            blend_mode,
            dest_rect,
            image_rotation,
        }
    }
}

impl CanvasBackend for ObservingCanvas {
    fn fill_path(
        &mut self,
        _path: &PdfPath,
        _fill_type: PathFillType,
        color: Color,
        _shader: &Option<Shader>,
        _blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        self.fill_colors.push(color);
        Ok(())
    }

    fn stroke_path(
        &mut self,
        _path: &PdfPath,
        _color: Color,
        _line_width: f32,
        stroke_style: &StrokeStyle,
        _shader: &Option<Shader>,
        _blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        self.stroke_styles.push(stroke_style.clone());
        Ok(())
    }

    fn set_clip_region(
        &mut self,
        _path: &PdfPath,
        _mode: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        Ok(())
    }

    fn width(&self) -> f32 {
        100.0
    }

    fn height(&self) -> f32 {
        100.0
    }

    fn save(&mut self) -> Result<(), PdfCanvasError> {
        self.save_count += 1;
        Ok(())
    }

    fn restore(&mut self) -> Result<(), PdfCanvasError> {
        self.restore_count += 1;
        Ok(())
    }

    fn draw_image_rect(
        &mut self,
        image: &Image,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError> {
        self.images.push(Self::observed_image(
            image,
            blend_mode,
            dest_rect,
            image_rotation,
        ));
        Ok(())
    }

    fn draw_inline_image(
        &mut self,
        image: &Image,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError> {
        self.inline_images.push(Self::observed_image(
            image,
            blend_mode,
            dest_rect,
            image_rotation,
        ));
        Ok(())
    }

    fn begin_mask_layer(
        &mut self,
        _mask: &Arc<RecordingCanvas>,
        _transform: &Transform,
        _mask_mode: MaskMode,
    ) -> Result<(), PdfCanvasError> {
        self.begin_mask_count += 1;
        Ok(())
    }

    fn end_mask_layer(
        &mut self,
        _mask: &Arc<RecordingCanvas>,
        _transform: &Transform,
        _mask_mode: MaskMode,
    ) -> Result<(), PdfCanvasError> {
        Ok(())
    }
}

#[allow(dead_code)]
pub fn content_stream(object_number: usize, data: &[u8]) -> ContentStream {
    let stream = StreamObject::new(
        object_number,
        0,
        Box::new(Dictionary::new(Default::default())),
        data.to_vec(),
    );
    let mut ids = ContentStreamIdAllocator::new();
    let mut content_stream = ContentStream::new(
        &ObjectVariant::Stream(stream),
        &PassthroughResolver,
        &mut ids,
    )
    .expect("content stream should parse");
    content_stream.id = object_number;
    content_stream
}

pub fn replay(recording: &RecordingCanvas) -> ObservingCanvas {
    let mut observer = ObservingCanvas::default();
    recording
        .replay(&mut observer)
        .expect("recorded commands should replay");
    observer
}
