//! Runtime registry for font format drivers.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::FontError;
use crate::font::{
    FontDriver, FontFace, FontFaceId, FontLoadRequest, FontMetadata, FontProgramFormat, FontSource,
};

/// Thread-safe registry used to select a font driver at runtime.
pub struct FontRegistry {
    drivers: Vec<Arc<dyn FontDriver>>,
    next_face_id: AtomicU64,
}

impl FontRegistry {
    /// Creates an empty driver registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            drivers: Vec::new(),
            next_face_id: AtomicU64::new(1),
        }
    }

    /// Registers a driver after all existing drivers in selection order.
    pub fn register(&mut self, driver: Arc<dyn FontDriver>) {
        self.drivers.push(driver);
    }

    /// Loads a face with the first driver accepting the request's format.
    ///
    /// The source and metadata are consumed into one [`FontLoadRequest`], so drivers can borrow the
    /// same request in selection order without cloning its bytes. Supporting drivers are tried
    /// until one succeeds; when all fail, the final driver error is returned. Face IDs are assigned
    /// before driver selection and remain unique even when a load attempt fails.
    pub fn load(
        &self,
        source: FontSource,
        metadata_hint: FontMetadata,
    ) -> Result<Arc<dyn FontFace>, FontError> {
        let format = Option::<FontProgramFormat>::from(&source).ok_or_else(|| {
            FontError::InvalidPdfSpecification {
                message: "an external font source requires a format hint".into(),
            }
        })?;
        let face_id = FontFaceId(self.next_face_id.fetch_add(1, Ordering::Relaxed));
        let request = FontLoadRequest {
            source,
            metadata_hint,
            face_id,
        };
        let mut accepted = false;
        let mut last_error = None;
        for driver in &self.drivers {
            if !driver.supports(format) {
                continue;
            }
            accepted = true;
            match driver.load(&request) {
                Ok(face) => return Ok(face),
                Err(error) => last_error = Some(error),
            }
        }
        if !accepted {
            return Err(FontError::DriverUnavailable { format });
        }
        Err(last_error.unwrap_or(FontError::DriverUnavailable { format }))
    }

    /// Returns the registered drivers in selection order.
    #[must_use]
    pub fn drivers(&self) -> &[Arc<dyn FontDriver>] {
        &self.drivers
    }
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new()
    }
}
