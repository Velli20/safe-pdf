use crate::{Jbig2Error, Result, document::DecodeBudget, error::LimitKind};

/// Boolean combination permitted by the applicable T.88 region/page flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombinationOperator {
    /// Set pixels present in either bitmap.
    Or,
    /// Set pixels present in both bitmaps.
    And,
    /// Set pixels present in exactly one bitmap.
    Xor,
    /// Set pixels equal in both bitmaps.
    Xnor,
    /// Replace destination pixels with source pixels.
    Replace,
}

impl TryFrom<u8> for CombinationOperator {
    type Error = Jbig2Error;

    /// Decode the three-bit external combination operator of a region segment
    /// (T.88 section 7.4.1.5), rejecting the reserved values five through seven.
    fn try_from(code: u8) -> Result<Self> {
        match code {
            0 => Ok(Self::Or),
            1 => Ok(Self::And),
            2 => Ok(Self::Xor),
            3 => Ok(Self::Xnor),
            4 => Ok(Self::Replace),
            _ => Err(Jbig2Error::InvalidResource("combination operator")),
        }
    }
}

impl CombinationOperator {
    /// Decode the two-bit default operator of a page information segment
    /// (T.88 section 7.4.8.5), which cannot select replacement.
    pub(crate) fn from_page_default(code: u8) -> Result<Self> {
        match code & 3 {
            0 => Ok(Self::Or),
            1 => Ok(Self::And),
            2 => Ok(Self::Xor),
            _ => Ok(Self::Xnor),
        }
    }
}

/// Region placement in page coordinates, allowing clipping at negative edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// Horizontal pixel offset.
    pub x: i64,
    /// Vertical pixel offset.
    pub y: i64,
    /// Effective operator after page override rules have been applied.
    pub operator: CombinationOperator,
}

/// Owned bilevel pixels: top-to-bottom rows, MSB-first bits, and one means black.
///
/// Storage has an explicit byte stride and zero unused row-tail bits. Foreign
/// adapters transfer bytes and dimensions, never this Rust struct's layout.
#[derive(Debug)]
pub struct Bitmap {
    width: u32,
    height: u32,
    stride: usize,
    bytes: Vec<u8>,
}

impl Bitmap {
    pub(crate) fn empty() -> Self {
        Self {
            width: 0,
            height: 0,
            stride: 0,
            bytes: Vec::new(),
        }
    }

    /// Allocate initialized pixels, checking dimensions and charging the budget.
    ///
    /// Returns geometry, overflow, budget, or allocation errors.
    pub fn new(width: u32, height: u32, black: bool, budget: &mut DecodeBudget) -> Result<Self> {
        let stride = usize::try_from(width)
            .map_err(|_| Jbig2Error::Overflow("bitmap width"))?
            .div_ceil(8);
        let len = stride
            .checked_mul(
                usize::try_from(height).map_err(|_| Jbig2Error::Overflow("bitmap height"))?,
            )
            .ok_or(Jbig2Error::Overflow("bitmap storage"))?;
        budget.charge(
            LimitKind::Pixels,
            u64::from(width)
                .checked_mul(u64::from(height))
                .ok_or(Jbig2Error::Overflow("bitmap pixels"))?,
        )?;
        budget.charge(
            LimitKind::AllocationBytes,
            u64::try_from(len).map_err(|_| Jbig2Error::Overflow("bitmap allocation"))?,
        )?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(len)?;
        bytes.resize(len, if black { 0xff } else { 0 });
        if black {
            let tail = tail_mask(width);
            for row in bytes.chunks_exact_mut(stride) {
                if let Some(last) = row.last_mut() {
                    *last &= tail;
                }
            }
        }
        Ok(Self {
            width,
            height,
            stride,
            bytes,
        })
    }

    /// Copy a borrowed bitmap into independent storage.
    ///
    /// A dictionary that re-exports a symbol it imported needs pixels that stay
    /// valid after the source dictionary is released.
    pub(crate) fn copy_of(source: BitmapRef<'_>, budget: &mut DecodeBudget) -> Result<Self> {
        let mut bitmap = Self::new(source.width(), source.height(), false, budget)?;
        for y in 0..source.height() {
            let start = usize::try_from(y)
                .map_err(|_| Jbig2Error::Overflow("pixel row"))?
                .checked_mul(source.stride())
                .ok_or(Jbig2Error::Overflow("pixel offset"))?;
            let end = start
                .checked_add(bitmap.stride)
                .ok_or(Jbig2Error::Overflow("pixel offset"))?;
            let row = source
                .bytes()
                .get(start..end)
                .ok_or(Jbig2Error::InvalidBitmap("source row"))?;
            bitmap.write_row(y, row)?;
        }
        Ok(bitmap)
    }

    /// Borrow a view whose lifetime cannot exceed this bitmap's storage.
    pub fn as_ref(&self) -> BitmapRef<'_> {
        BitmapRef {
            width: self.width,
            height: self.height,
            stride: self.stride,
            bytes: &self.bytes,
        }
    }

    /// Overwrite one row from tightly packed, most-significant-bit-first bytes.
    ///
    /// Unused tail bits are cleared, so storage stays canonical whatever the
    /// source row's padding contained.
    pub(crate) fn write_row(&mut self, y: u32, source: &[u8]) -> Result<()> {
        if y >= self.height {
            return Err(Jbig2Error::InvalidBitmap("row outside bitmap"));
        }
        let start = usize::try_from(y)
            .map_err(|_| Jbig2Error::Overflow("pixel row"))?
            .checked_mul(self.stride)
            .ok_or(Jbig2Error::Overflow("pixel offset"))?;
        let end = start
            .checked_add(self.stride)
            .ok_or(Jbig2Error::Overflow("pixel offset"))?;
        let row = self
            .bytes
            .get_mut(start..end)
            .ok_or(Jbig2Error::InvalidBitmap("row storage"))?;
        if source.len() < row.len() {
            return Err(Jbig2Error::InvalidBitmap("row source is too short"));
        }
        for (destination, byte) in row.iter_mut().zip(source.iter().copied()) {
            *destination = byte;
        }
        if let Some(last) = row.last_mut() {
            *last &= tail_mask(self.width);
        }
        Ok(())
    }

    /// Transfer packed storage to an embedding application's output adapter.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// Repack as tightly packed rows in PDF polarity, where a clear bit is black.
    ///
    /// Stride padding is dropped and every byte inverted, so the canonical zero
    /// tail bits of a final partial byte become one bits, as PDF expects. Raw
    /// `into_bytes` keeps the core convention and never converts polarity.
    pub fn to_pdf_bytes(&self, budget: &mut DecodeBudget) -> Result<Vec<u8>> {
        let row_bytes = usize::try_from(self.width)
            .map_err(|_| Jbig2Error::Overflow("PDF width"))?
            .div_ceil(8);
        let rows = usize::try_from(self.height).map_err(|_| Jbig2Error::Overflow("PDF height"))?;
        let len = row_bytes
            .checked_mul(rows)
            .ok_or(Jbig2Error::Overflow("PDF image storage"))?;
        budget.charge(
            LimitKind::AllocationBytes,
            u64::try_from(len).map_err(|_| Jbig2Error::Overflow("PDF allocation"))?,
        )?;
        let mut output = Vec::new();
        output.try_reserve_exact(len)?;
        let tail = tail_mask(self.width);
        for row in self.bytes.chunks_exact(self.stride) {
            let pixels = row
                .get(..row_bytes)
                .ok_or(Jbig2Error::InvalidBitmap("PDF source row"))?;
            let last = row_bytes.saturating_sub(1);
            for (column, &byte) in pixels.iter().enumerate() {
                let masked = if column == last { byte & tail } else { byte };
                output.push(!masked);
            }
        }
        Ok(output)
    }
}

/// Mask retaining only the pixel bits of a row's final byte.
fn tail_mask(width: u32) -> u8 {
    if width.is_multiple_of(8) {
        0xff
    } else {
        0xff_u8 << 8_u32.saturating_sub(width % 8)
    }
}

/// Validated immutable pixels borrowed from Rust-owned or adapter-owned storage.
///
/// The buffer must remain alive and immovable from the adapter's perspective.
/// Dimensions and stride are validated without copying any pixel bytes.
///
/// Views cannot escape their backing allocation:
/// ```compile_fail
/// use pdf_jbig2::bitmap::BitmapRef;
/// fn dangling() -> BitmapRef<'static> {
///     let bytes = vec![0_u8];
///     BitmapRef::new(8, 1, 1, &bytes).unwrap()
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct BitmapRef<'a> {
    width: u32,
    height: u32,
    stride: usize,
    bytes: &'a [u8],
}

impl<'a> BitmapRef<'a> {
    /// Validate row extents and stride; unused row-tail bits are ignored by pixel operations.
    pub fn new(width: u32, height: u32, stride: usize, bytes: &'a [u8]) -> Result<Self> {
        let min_stride = usize::try_from(width)
            .map_err(|_| Jbig2Error::Overflow("bitmap width"))?
            .div_ceil(8);
        if stride < min_stride {
            return Err(Jbig2Error::InvalidBitmap(
                "stride is shorter than the row width",
            ));
        }
        let expected = stride
            .checked_mul(
                usize::try_from(height).map_err(|_| Jbig2Error::Overflow("bitmap height"))?,
            )
            .ok_or(Jbig2Error::Overflow("bitmap storage"))?;
        if bytes.len() != expected {
            return Err(Jbig2Error::InvalidBitmap(
                "storage length does not match geometry",
            ));
        }
        Ok(Self {
            width,
            height,
            stride,
            bytes,
        })
    }

    /// Return pixel width without narrowing the T.88 dimension.
    pub fn width(self) -> u32 {
        self.width
    }

    /// Return pixel height.
    pub fn height(self) -> u32 {
        self.height
    }

    /// Return the byte distance between consecutive row starts.
    pub fn stride(self) -> usize {
        self.stride
    }

    /// Borrow the validated storage for the original buffer lifetime.
    pub fn bytes(self) -> &'a [u8] {
        self.bytes
    }
}

/// Safe extension boundary for clipping and combining decoded region pixels.
///
/// Implementations must preserve bitmap geometry and row-tail invariants, clip
/// against the destination, and charge work to the supplied budget. They must
/// not retain either bitmap borrow or expose it beyond an FFI/WASM call.
///
/// An adapter can supply a Rust implementation without changing codec stages:
/// ```no_run
/// use pdf_jbig2::{Result, bitmap::{Bitmap, BitmapCompositor, BitmapRef,
///     DefaultCompositor, Placement}, document::DecodeBudget};
/// struct AdapterCompositor;
/// impl BitmapCompositor for AdapterCompositor {
///     fn compose(&mut self, target: &mut Bitmap, source: BitmapRef<'_>,
///         placement: Placement, budget: &mut DecodeBudget) -> Result<()> {
///         DefaultCompositor.compose(target, source, placement, budget)
///     }
/// }
/// ```
pub trait BitmapCompositor {
    /// Apply the effective combination operator, reporting budget/geometry errors.
    fn compose(
        &mut self,
        destination: &mut Bitmap,
        source: BitmapRef<'_>,
        placement: Placement,
        budget: &mut DecodeBudget,
    ) -> Result<()>;
}

/// Portable scalar implementation of the composition contract.
#[derive(Debug, Default)]
pub struct DefaultCompositor;

impl BitmapCompositor for DefaultCompositor {
    fn compose(
        &mut self,
        destination: &mut Bitmap,
        source: BitmapRef<'_>,
        placement: Placement,
        budget: &mut DecodeBudget,
    ) -> Result<()> {
        let work = u64::from(source.width)
            .checked_mul(u64::from(source.height))
            .ok_or(Jbig2Error::Overflow("composition work"))?;
        budget.charge(LimitKind::Work, work)?;
        for sy in 0..source.height {
            let dy = placement.y.checked_add(i64::from(sy));
            let Some(dy) = dy.and_then(|value| u32::try_from(value).ok()) else {
                continue;
            };
            if dy >= destination.height {
                continue;
            }
            for sx in 0..source.width {
                let dx = placement.x.checked_add(i64::from(sx));
                let Some(dx) = dx.and_then(|value| u32::try_from(value).ok()) else {
                    continue;
                };
                if dx >= destination.width {
                    continue;
                }
                let src = source
                    .pixel(sx, sy)
                    .ok_or(Jbig2Error::InvalidBitmap("source pixel"))?;
                let dst = destination
                    .as_ref()
                    .pixel(dx, dy)
                    .ok_or(Jbig2Error::InvalidBitmap("destination pixel"))?;
                let value = match placement.operator {
                    CombinationOperator::Or => dst | src,
                    CombinationOperator::And => dst & src,
                    CombinationOperator::Xor => dst ^ src,
                    CombinationOperator::Xnor => dst == src,
                    CombinationOperator::Replace => src,
                };
                destination.set_pixel(dx, dy, value)?;
            }
        }
        Ok(())
    }
}

impl Bitmap {
    pub(crate) fn set_pixel(&mut self, x: u32, y: u32, black: bool) -> Result<()> {
        let index = usize::try_from(y)
            .map_err(|_| Jbig2Error::Overflow("pixel row"))?
            .checked_mul(self.stride)
            .and_then(|base| base.checked_add(usize::try_from(x / 8).ok()?))
            .ok_or(Jbig2Error::Overflow("pixel offset"))?;
        if x >= self.width || y >= self.height {
            return Err(Jbig2Error::InvalidBitmap("pixel outside bitmap"));
        }
        let byte = self
            .bytes
            .get_mut(index)
            .ok_or(Jbig2Error::InvalidBitmap("pixel storage"))?;
        let mask = 0x80_u8 >> (x % 8);
        if black {
            *byte |= mask;
        } else {
            *byte &= !mask;
        }
        Ok(())
    }
}

impl BitmapRef<'_> {
    pub(crate) fn pixel(self, x: u32, y: u32) -> Option<bool> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let row = usize::try_from(y).ok()?.checked_mul(self.stride)?;
        let index = row.checked_add(usize::try_from(x / 8).ok()?)?;
        self.bytes
            .get(index)
            .map(|byte| byte & (0x80_u8 >> (x % 8)) != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Bitmap, BitmapCompositor, BitmapRef, CombinationOperator, DefaultCompositor, Placement,
    };
    use crate::{
        Result,
        document::{DecodeBudget, DecodeLimits},
    };

    fn budget() -> DecodeBudget {
        DecodeBudget::new(DecodeLimits {
            input_bytes: 1024,
            segments: 1024,
            references: 1024,
            pixels: 1024,
            allocation_bytes: 1024,
            symbols: 1024,
            work: 1024,
        })
    }

    #[test]
    fn black_fill_masks_unused_tail_bits() -> Result<()> {
        let mut budget = budget();
        let bitmap = Bitmap::new(9, 2, true, &mut budget)?;
        assert_eq!(bitmap.as_ref().bytes(), &[0xff, 0x80, 0xff, 0x80]);
        assert!(BitmapRef::new(9, 1, 1, &[0xff]).is_err());
        assert!(BitmapRef::new(9, 1, 2, &[0xff]).is_err());
        Ok(())
    }

    #[test]
    fn composition_clips_and_applies_every_operator() -> Result<()> {
        let mut budget = budget();
        let source = BitmapRef::new(2, 1, 1, &[0x80])?;
        let cases = [
            (CombinationOperator::Or, 0xc0),
            (CombinationOperator::And, 0x80),
            (CombinationOperator::Xor, 0x40),
            (CombinationOperator::Xnor, 0x80),
            (CombinationOperator::Replace, 0x80),
        ];
        for (operator, expected) in cases {
            let mut dest = Bitmap::new(2, 1, false, &mut budget)?;
            dest.write_row(0, &[0xc0])?;
            DefaultCompositor.compose(
                &mut dest,
                source,
                Placement {
                    x: 0,
                    y: 0,
                    operator,
                },
                &mut budget,
            )?;
            assert_eq!(dest.as_ref().bytes(), &[expected]);
        }
        let mut clipped = Bitmap::new(1, 1, false, &mut budget)?;
        DefaultCompositor.compose(
            &mut clipped,
            source,
            Placement {
                x: -1,
                y: 0,
                operator: CombinationOperator::Replace,
            },
            &mut budget,
        )?;
        assert_eq!(clipped.as_ref().bytes(), &[0]);
        Ok(())
    }
}
