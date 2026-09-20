//! Validated soft-mask metadata and explicit, ownership-based completion.
#![deny(missing_docs, clippy::missing_docs_in_private_items)]

use crate::{
    error::PdfCanvasError,
    recording_canvas::{MAX_NESTING, RecordingCanvas},
};
use pdf_graphics::{Image, MaskMode, transform::Transform};
use std::sync::Arc;

/// Invalid mask geometry or an unfinished recording.
#[derive(Debug, thiserror::Error)]
pub enum MaskError {
    /// The mask recording contains invalid save or body boundaries.
    #[error(transparent)]
    Recording(#[from] crate::recording_canvas::RecordingError),
    /// Recording dimensions must be positive and finite.
    #[error("invalid mask dimensions: {width} x {height}")]
    InvalidDimensions {
        /// Supplied logical width.
        width: f32,
        /// Supplied logical height.
        height: f32,
    },
    /// The mask-to-device mapping contains a nonfinite component.
    #[error(transparent)]
    Transform(#[from] pdf_graphics::transform::TransformError),
    /// The recording nests more masks or patterns than backends replay.
    #[error("mask nesting exceeds {MAX_NESTING} levels")]
    NestingLimit,
}

/// A backend raster that can hold replayed mask content and processed coverage.
///
/// Implementors choose the raster space; [`MaskLayer::render_coverage`] only sequences
/// replay, portable pixel processing, and upload.
pub trait CoverageTarget {
    /// Backend-owned raster holding coverage pixels.
    type Raster;

    /// Replays the mask recording into a fresh raster.
    fn replay_mask(&mut self, mask: &MaskLayer) -> Result<Self::Raster, PdfCanvasError>;

    /// Reads a raster back as straight-alpha pixels, releasing the raster.
    fn read(&mut self, raster: Self::Raster) -> Result<Image, PdfCanvasError>;

    /// Uploads processed coverage pixels into a new raster.
    fn upload(&mut self, image: &Image) -> Result<Self::Raster, PdfCanvasError>;
}

/// Immutable mask content and its mapping into logical device coordinates.
///
/// Clones share recordings and transfer tables. No backend resource is retained.
#[derive(Clone)]
pub struct MaskLayer {
    /// Drawing commands in mask coordinates.
    recording: Arc<RecordingCanvas>,
    /// Mask coordinates to logical device coordinates, before backend scaling.
    transform: Transform,
    /// Selection of alpha, luminosity, or pass-through behavior.
    mode: MaskMode,
    /// Optional mapping applied after computing coverage.
    transfer: Option<Arc<[u8; 256]>>,
}

impl MaskLayer {
    /// Validates dimensions, mapping, and recording balance without requiring an inverse.
    ///
    /// # Errors
    /// Rejects nonfinite transforms, nonpositive/nonfinite dimensions, open recording
    /// scopes, or recordings nested deeper than [`MAX_NESTING`].
    pub fn new(
        recording: Arc<RecordingCanvas>,
        transform: Transform,
        mode: MaskMode,
        transfer: Option<Arc<[u8; 256]>>,
    ) -> Result<Self, MaskError> {
        transform.validate()?;
        if recording.nesting() >= MAX_NESTING {
            return Err(MaskError::NestingLimit);
        }
        if [recording.width, recording.height]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
        {
            return Err(MaskError::InvalidDimensions {
                width: recording.width,
                height: recording.height,
            });
        }
        recording.validate()?;
        Ok(Self {
            recording,
            transform,
            mode,
            transfer,
        })
    }

    /// Returns the shared recording without copying drawing commands.
    pub fn recording(&self) -> &Arc<RecordingCanvas> {
        &self.recording
    }

    /// Returns the mask-to-logical-device mapping.
    pub fn transform(&self) -> &Transform {
        &self.transform
    }

    /// Returns the coverage interpretation.
    pub fn mode(&self) -> &MaskMode {
        &self.mode
    }

    /// Returns the optional post-coverage lookup table.
    pub fn transfer(&self) -> Option<&[u8; 256]> {
        self.transfer.as_deref()
    }

    /// Whether this descriptor represents an unsupported mode that passes content through.
    pub fn is_passthrough(&self) -> bool {
        matches!(self.mode, MaskMode::Unknown(_))
    }

    /// Whether native alpha coverage requires portable pixel processing.
    pub fn needs_processing(&self) -> bool {
        self.mode == MaskMode::Luminosity || self.transfer.is_some()
    }

    /// Renders coverage into a backend raster, processing pixels only when required.
    ///
    /// Alpha masks without a transfer table use the replayed raster directly; other
    /// modes read it back, apply [`Self::prepare_coverage`], and upload the result.
    pub fn render_coverage<T: CoverageTarget>(
        &self,
        target: &mut T,
    ) -> Result<T::Raster, PdfCanvasError> {
        let raster = target.replay_mask(self)?;
        if !self.needs_processing() {
            return Ok(raster);
        }
        let image = target.read(raster)?;
        let coverage = self.prepare_coverage(&image)?;
        target.upload(&coverage)
    }

    /// Prepares coverage from straight-alpha pixels, applying luminosity before transfer.
    ///
    /// Callers enforce budgets and transformed bounds; pixels outside the mask bounds must
    /// remain excluded even if the transfer table maps zero to nonzero.
    pub fn prepare_coverage(&self, image: &Image) -> Result<Image, pdf_image::ImageRasterError> {
        if self.is_passthrough() {
            return Err(pdf_image::ImageRasterError::InvalidInput(
                "pass-through mask has no coverage",
            ));
        }
        pdf_image::raster::validate_image(image)?;
        let coverage = if self.mode == MaskMode::Luminosity {
            pdf_image::raster::luminosity_mask(image)?
        } else {
            image.clone()
        };
        Ok(if let Some(table) = self.transfer() {
            pdf_image::raster::transfer_mask(&coverage, table)?
        } else {
            coverage
        })
    }
}

#[cfg(test)]
mod tests {
    //! Descriptor validation and exact portable coverage semantics.
    use super::*;
    use pdf_graphics::PixelFormat;

    /// Validates geometry without imposing invertibility and rejects unfinished recordings.
    #[test]
    fn validates_geometry_and_recording_balance() {
        let recording = Arc::new(RecordingCanvas::new(1.0, 1.0));
        assert!(
            MaskLayer::new(
                Arc::clone(&recording),
                Transform::from_scale(0.0, 1.0),
                MaskMode::Alpha,
                None
            )
            .is_ok()
        );
        assert!(
            MaskLayer::new(
                recording,
                Transform::from_scale(f32::NAN, 1.0),
                MaskMode::Alpha,
                None
            )
            .is_err()
        );
        for width in [0.0, -1.0, f32::INFINITY] {
            assert!(
                MaskLayer::new(
                    Arc::new(RecordingCanvas::new(width, 1.0)),
                    Transform::identity(),
                    MaskMode::Alpha,
                    None
                )
                .is_err()
            );
        }
        let mut recording = RecordingCanvas::new(1.0, 1.0);
        crate::canvas_backend::CanvasBackend::save(&mut recording).unwrap();
        assert!(
            MaskLayer::new(
                Arc::new(recording),
                Transform::identity(),
                MaskMode::Alpha,
                None
            )
            .is_err()
        );
    }
    /// Checks alpha, straight-RGB luminosity, lookup ordering, and malformed input.
    #[test]
    fn coverage_matches_explicit_bytes() {
        let image = Image {
            data: vec![255, 0, 0, 0, 0, 255, 0, 128].into(),
            width: 2,
            height: 1,
            pixel_format: PixelFormat::RGBA8888,
        };
        let recording = Arc::new(RecordingCanvas::new(2.0, 1.0));
        let alpha = MaskLayer::new(
            Arc::clone(&recording),
            Transform::identity(),
            MaskMode::Alpha,
            None,
        )
        .unwrap();
        assert!(!alpha.needs_processing());
        assert_eq!(alpha.prepare_coverage(&image).unwrap(), image);
        let luma = MaskLayer::new(
            Arc::clone(&recording),
            Transform::identity(),
            MaskMode::Luminosity,
            None,
        )
        .unwrap();
        assert_eq!(
            luma.prepare_coverage(&image).unwrap().data.as_ref(),
            &[255, 255, 255, 76, 255, 255, 255, 149]
        );
        let table = Arc::new(std::array::from_fn(|i| {
            255_u8.saturating_sub(u8::try_from(i).unwrap())
        }));
        let transferred = MaskLayer::new(
            Arc::clone(&recording),
            Transform::identity(),
            MaskMode::Luminosity,
            Some(Arc::clone(&table)),
        )
        .unwrap();
        assert_eq!(
            transferred.prepare_coverage(&image).unwrap().data.as_ref(),
            &[255, 255, 255, 179, 255, 255, 255, 106]
        );
        let alpha = MaskLayer::new(
            recording,
            Transform::identity(),
            MaskMode::Alpha,
            Some(table),
        )
        .unwrap();
        assert_eq!(
            alpha.prepare_coverage(&image).unwrap().data.as_ref(),
            &[255, 255, 255, 255, 255, 255, 255, 127]
        );
        let malformed = Image { width: 3, ..image };
        assert!(alpha.prepare_coverage(&malformed).is_err());
    }
}
