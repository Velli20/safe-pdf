//! Browser-backed temporary rendering surfaces.

use crate::{
    budget::Reservation,
    error::{WebCanvasBackendError as Error, WebResult},
};
use wasm_bindgen::JsCast;

/// A temporary Canvas 2D surface that retains its storage-budget reservation.
pub(crate) struct Surface {
    canvas: web_sys::HtmlCanvasElement,
    context: web_sys::CanvasRenderingContext2d,
    _reservation: Reservation,
}

impl Surface {
    /// Creates a browser surface with the supplied dimensions and budget reservation.
    pub(crate) fn new(size: [u32; 2], reservation: Reservation) -> WebResult<Self> {
        let canvas: web_sys::HtmlCanvasElement = document()?
            .create_element("canvas")?
            .dyn_into()
            .map_err(|_| Error::ContextUnavailable)?;
        let [w, h] = size;
        canvas.set_width(w);
        canvas.set_height(h);
        let context = context(&canvas)?;
        Ok(Self {
            canvas,
            context,
            _reservation: reservation,
        })
    }

    /// Returns the browser canvas holding this surface's pixels.
    pub(crate) fn canvas(&self) -> &web_sys::HtmlCanvasElement {
        &self.canvas
    }

    /// Returns the Canvas 2D context associated with this surface.
    pub(crate) fn context(&self) -> &web_sys::CanvasRenderingContext2d {
        &self.context
    }

    /// Returns this surface's dimensions in backing pixels.
    pub(crate) fn size(&self) -> [u32; 2] {
        [self.canvas.width(), self.canvas.height()]
    }
}

impl Drop for Surface {
    /// Releases this surface's browser backing storage and budget reservation.
    fn drop(&mut self) {
        self.canvas.set_width(0);
        self.canvas.set_height(0);
    }
}

/// Obtains the browser document used to create temporary canvases.
pub(crate) fn document() -> WebResult<web_sys::Document> {
    web_sys::window()
        .and_then(|window| window.document())
        .ok_or(Error::ContextUnavailable)
}

/// Obtains the Canvas 2D context associated with a browser canvas.
pub(crate) fn context(
    canvas: &web_sys::HtmlCanvasElement,
) -> WebResult<web_sys::CanvasRenderingContext2d> {
    canvas
        .get_context("2d")?
        .ok_or(Error::ContextUnavailable)?
        .dyn_into()
        .map_err(|_| Error::ContextUnavailable)
}
