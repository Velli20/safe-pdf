//! Shared, bounded storage for recursive browser rendering.
use crate::{
    budget::{Budget, BudgetedImage, Reservation},
    error::{WebCanvasBackendError as Error, WebResult},
    surface::Surface,
};
use pdf_graphics::{Image, PixelFormat};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::Clamped;

/// Reusable temporary canvases and shared pixel-storage accounting for recursive rendering.
///
/// Clones share both idle surfaces and the budget. Checked-out surfaces, idle surfaces,
/// readback images, and scratch reservations all remain charged until released.
/// The limit covers accounted pixel storage, not total browser or process memory.
#[derive(Clone)]
pub(crate) struct SurfacePool {
    /// Idle surfaces that can be reused or evicted to satisfy a reservation.
    available: Rc<RefCell<Vec<Surface>>>,
    /// Accounting shared with resources, independently of the pool to avoid ownership cycles.
    budget: Budget,
}

impl SurfacePool {
    /// Creates an empty pool with a limit in accounted pixel bytes.
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            available: Rc::new(RefCell::new(Vec::new())),
            budget: Budget::new(limit),
        }
    }

    /// Reuses a matching temporary surface or allocates one within the shared budget.
    pub(crate) fn acquire(&self, size: [u32; 2]) -> WebResult<Surface> {
        let found = self
            .available
            .borrow()
            .iter()
            .position(|s| s.size() == size);
        if let Some(index) = found {
            let surface = self.available.borrow_mut().swap_remove(index);
            return Ok(surface);
        }
        let bytes = byte_size(size)?;
        let reservation = self.scratch(bytes)?;
        Surface::new(size, reservation)
    }

    /// Resets the surface and returns it to the pool for later reuse.
    pub(crate) fn recycle(&self, surface: Surface) {
        surface.canvas().set_width(surface.canvas().width());
        self.available.borrow_mut().push(surface);
    }

    /// Releases surfaces retained by the pool.
    pub(crate) fn clear(&self) {
        self.available.borrow_mut().clear();
    }

    /// Reserves temporary bytes until the returned reservation is released.
    /// Evicts idle surfaces one at a time only when the request needs more space.
    pub(crate) fn scratch(&self, bytes: usize) -> WebResult<Reservation> {
        if bytes > self.budget.limit() {
            return Err(Error::ResourceLimit);
        }
        loop {
            match self.budget.reserve(bytes) {
                Ok(reservation) => return Ok(reservation),
                Err(error) => {
                    let Some(surface) = self.available.borrow_mut().pop() else {
                        return Err(error);
                    };
                    drop(surface);
                }
            }
        }
    }

    /// Copies a surface into an image that retains its pixel-storage reservation.
    ///
    /// Accounts for both the browser ImageData and Rust copy during readback, then
    /// releases the transient charge. Browser and budget errors release all new charges.
    pub(crate) fn read(&self, surface: &Surface) -> WebResult<BudgetedImage> {
        let size = surface.size();
        let bytes = byte_size(size)?;
        let reservation = self.scratch(bytes)?;
        let _scratch = self.scratch(bytes)?;
        let [w, h] = size;
        let data = surface
            .context()
            .get_image_data(0.0, 0.0, f64::from(w), f64::from(h))?;
        let image = Image {
            width: usize::try_from(w).map_err(|_| Error::ResourceLimit)?,
            height: usize::try_from(h).map_err(|_| Error::ResourceLimit)?,
            pixel_format: PixelFormat::RGBA8888,
            data: data.data().0.into(),
        };
        Ok(BudgetedImage::new(image, reservation))
    }

    /// Uploads validated straight RGBA pixels into a budgeted browser surface.
    ///
    /// Reserves temporary storage before conversion and propagates image, budget,
    /// and browser failures. The returned surface retains its own storage reservation.
    pub(crate) fn upload(&self, image: &Image) -> WebResult<Surface> {
        let size = image.dimensions().ok_or(Error::ResourceLimit)?;
        byte_size(size)?;
        pdf_image::raster::validate_image(image)?;

        // Account for the Rust conversion buffer and browser ImageData copy before
        // allocating either, and retain that reservation until upload has completed.
        let _scratch = self.scratch(
            byte_size(size)?
                .checked_mul(2)
                .ok_or(Error::ResourceLimit)?,
        )?;
        let surface = self.acquire(size)?;
        let [w, h] = size;
        let data = pdf_image::raster::rgba(image)?;
        let data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(&data), w, h)?;
        surface.context().put_image_data(&data, 0.0, 0.0)?;
        Ok(surface)
    }
}

/// Returns RGBA surface storage size, rejecting empty dimensions and byte-count overflow.
pub(crate) fn byte_size(size: [u32; 2]) -> WebResult<usize> {
    let [w, h] = size;
    if w == 0 || h == 0 {
        return Err(Error::InvalidInput("empty surface"));
    }
    usize::try_from(w)
        .map_err(|_| Error::ResourceLimit)?
        .checked_mul(usize::try_from(h).map_err(|_| Error::ResourceLimit)?)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(Error::ResourceLimit)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_budget_accepts_only_empty_reservations() {
        let pool = SurfacePool::new(0);
        let _empty = pool.scratch(0).unwrap();
        assert!(pool.scratch(1).is_err());
        assert_eq!(pool.budget.used(), 0);
    }

    #[test]
    fn image_clones_retain_one_charge_after_pool_drop() {
        let pool = SurfacePool::new(4);
        let budget = pool.budget.clone();
        let image = BudgetedImage::new(
            Image {
                width: 1,
                height: 1,
                pixel_format: PixelFormat::RGBA8888,
                data: vec![0; 4].into(),
            },
            pool.scratch(4).unwrap(),
        );
        let clone = image.clone();
        drop(pool);
        drop(image);
        assert_eq!(budget.used(), 4);
        assert_eq!(clone.image().data.len(), 4);
        drop(clone);
        assert_eq!(budget.used(), 0);
    }
    #[test]
    /// Verifies that rejects empty and overflowing surfaces.
    fn rejects_empty_and_overflowing_surfaces() {
        assert!(byte_size([0, 4]).is_err());
        assert!(byte_size([u32::MAX, u32::MAX]).is_err());
        assert_eq!(byte_size([2, 3]).unwrap(), 24);
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod browser_tests {
    use super::*;
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn impossible_request_preserves_cached_surfaces() {
        let pool = SurfacePool::new(8);
        let surface = pool.acquire([1, 1]).unwrap();
        let canvas = surface.canvas().clone();
        pool.recycle(surface);
        assert!(pool.scratch(9).is_err());
        assert_eq!(pool.available.borrow().len(), 1);
        assert_eq!(canvas.width(), 1);
        assert_eq!(pool.budget.used(), 4);
    }

    #[wasm_bindgen_test]
    fn eviction_stops_when_request_fits() {
        let pool = SurfacePool::new(12);
        let first = pool.acquire([1, 1]).unwrap();
        let second = pool.acquire([2, 1]).unwrap();
        let first_canvas = first.canvas().clone();
        let second_canvas = second.canvas().clone();
        pool.recycle(first);
        pool.recycle(second);
        let scratch = pool.scratch(8).unwrap();
        assert_eq!(pool.available.borrow().len(), 1);
        assert_eq!(first_canvas.width(), 1);
        assert_eq!(second_canvas.width(), 0);
        assert_eq!(pool.budget.used(), 12);
        drop(scratch);
        let reused = pool.acquire([1, 1]).unwrap();
        assert_eq!(reused.canvas(), &first_canvas);
        assert_eq!(pool.budget.used(), 4);
    }

    #[wasm_bindgen_test]
    fn readback_accounts_for_peak_and_retained_pixels() {
        let pool = SurfacePool::new(12);
        let surface = pool.acquire([1, 1]).unwrap();
        let image = pool.read(&surface).unwrap();
        assert_eq!(image.image().data.as_ref(), &[0, 0, 0, 0]);
        assert_eq!(pool.budget.used(), 8);
        drop(image);
        assert_eq!(pool.budget.used(), 4);

        let tight = SurfacePool::new(11);
        let surface = tight.acquire([1, 1]).unwrap();
        assert!(matches!(tight.read(&surface), Err(Error::ResourceLimit)));
        assert_eq!(tight.budget.used(), 4);
    }

    #[wasm_bindgen_test]
    fn browser_readback_error_releases_new_charges() {
        let pool = SurfacePool::new(12);
        let surface = pool.acquire([1, 1]).unwrap();
        // Force a browser exception after readback reservations have been acquired.
        js_sys::Reflect::set(
            surface.context().as_ref(),
            &"getImageData".into(),
            &js_sys::Function::new_no_args("throw new Error('readback failure')"),
        )
        .unwrap();
        assert!(matches!(pool.read(&surface), Err(Error::Browser(_))));
        assert_eq!(pool.budget.used(), 4);
    }
}
