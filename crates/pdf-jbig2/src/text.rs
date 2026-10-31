//! Shared symbol placement for text regions and dictionary aggregation.
//!
//! The strip decoding procedure of T.88 section 6.4.5 is written once here.
//! Arithmetic text regions, Huffman text regions, and the refinement and
//! aggregation path of a symbol dictionary all drive that one loop through
//! [`TextSymbolSource`], supplying only their entropy-specific integer reads.

use crate::{
    Jbig2Error, Result,
    bitmap::{Bitmap, BitmapCompositor, BitmapRef, CombinationOperator, Placement},
    document::DecodeBudget,
    error::LimitKind,
    region::ReferenceCorner,
};

impl TryFrom<u8> for ReferenceCorner {
    type Error = Jbig2Error;

    /// Decode the two-bit `REFCORNER` field (T.88 section 7.4.3.1.1).
    fn try_from(code: u8) -> Result<Self> {
        match code & 3 {
            0 => Ok(Self::BottomLeft),
            1 => Ok(Self::TopLeft),
            2 => Ok(Self::BottomRight),
            _ => Ok(Self::TopRight),
        }
    }
}

impl ReferenceCorner {
    /// Whether this corner anchors the symbol at its right edge.
    fn is_right(self) -> bool {
        matches!(self, Self::TopRight | Self::BottomRight)
    }

    /// Whether this corner anchors the symbol at its bottom edge.
    fn is_bottom(self) -> bool {
        matches!(self, Self::BottomLeft | Self::BottomRight)
    }
}

/// Placement parameters shared by every instance of one text decoding run.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TextLayout {
    /// Number of symbol instances the run must place.
    pub(crate) instances: u32,
    /// Rows per strip, `SBSTRIPS`.
    pub(crate) strip_size: u32,
    /// Corner of the symbol that the decoded coordinates address.
    pub(crate) reference_corner: ReferenceCorner,
    /// Whether the S and T axes are exchanged.
    pub(crate) transposed: bool,
    /// Operator combining each symbol into the region bitmap.
    pub(crate) operator: CombinationOperator,
    /// Signed offset added to every subsequent-symbol S delta.
    pub(crate) delta_s_offset: i8,
}

impl TextLayout {
    /// Shift `CURS` to the symbol's near edge before placement
    /// (T.88 section 6.4.5 step 3(c)v).
    fn adjust_before(self, curs: i64, width: u32, height: u32) -> Result<i64> {
        let advance = if self.transposed {
            self.reference_corner.is_bottom()
        } else {
            self.reference_corner.is_right()
        };
        if !advance {
            return Ok(curs);
        }
        let extent = if self.transposed { height } else { width };
        curs.checked_add(extent_minus_one(extent)?)
            .ok_or(Jbig2Error::Overflow("text placement"))
    }

    /// Advance `CURS` past the placed symbol (T.88 section 6.4.5 step 3(c)x).
    fn advance_after(self, curs: i64, width: u32, height: u32) -> Result<i64> {
        let advance = if self.transposed {
            !self.reference_corner.is_bottom()
        } else {
            !self.reference_corner.is_right()
        };
        if !advance {
            return Ok(curs);
        }
        let extent = if self.transposed { height } else { width };
        curs.checked_add(extent_minus_one(extent)?)
            .ok_or(Jbig2Error::Overflow("text placement"))
    }

    /// Map decoded `(S, T)` coordinates to the symbol's top-left corner.
    ///
    /// Transposition exchanges which decoded coordinate drives each axis; the
    /// reference corner then moves the anchor back across the symbol.
    fn placement(self, s: i64, t: i64, width: u32, height: u32) -> Result<(i64, i64)> {
        let (mut x, mut y) = if self.transposed { (t, s) } else { (s, t) };
        if self.reference_corner.is_right() {
            x = x
                .checked_sub(extent_minus_one(width)?)
                .ok_or(Jbig2Error::Overflow("text placement"))?;
        }
        if self.reference_corner.is_bottom() {
            y = y
                .checked_sub(extent_minus_one(height)?)
                .ok_or(Jbig2Error::Overflow("text placement"))?;
        }
        Ok((x, y))
    }
}

/// Return a symbol extent minus one, which placement uses as its back edge.
fn extent_minus_one(extent: u32) -> Result<i64> {
    i64::from(extent)
        .checked_sub(1)
        .ok_or(Jbig2Error::Overflow("symbol extent"))
}

/// Working variables of the strip procedure (T.88 section 6.4.5).
#[derive(Debug, Clone, Copy)]
struct StripState {
    /// Absolute T coordinate of the current strip.
    strip_top: i64,
    /// Running first-symbol S coordinate across strips.
    first_s: i64,
    /// S coordinate of the instance being placed.
    current_s: i64,
    /// Instances placed so far.
    placed: u32,
}

impl StripState {
    /// Initialize from the leading strip delta (T.88 section 6.4.5 step 2).
    fn begin(initial_delta_t: i32, strip_size: u32) -> Result<Self> {
        let strip_top = scale_strip_delta(initial_delta_t, strip_size)?
            .checked_neg()
            .ok_or(Jbig2Error::Overflow("text strip origin"))?;
        Ok(Self {
            strip_top,
            first_s: 0,
            current_s: 0,
            placed: 0,
        })
    }

    /// Move to the next strip (T.88 section 6.4.5 step 3(b)).
    fn begin_strip(&mut self, delta_t: i32, strip_size: u32) -> Result<()> {
        self.strip_top = self
            .strip_top
            .checked_add(scale_strip_delta(delta_t, strip_size)?)
            .ok_or(Jbig2Error::Overflow("text strip position"))?;
        Ok(())
    }

    /// Apply this strip's first-symbol S delta (T.88 section 6.4.7).
    fn take_first_s(&mut self, delta: i32) -> Result<()> {
        self.first_s = self
            .first_s
            .checked_add(i64::from(delta))
            .ok_or(Jbig2Error::Overflow("text first S"))?;
        self.current_s = self.first_s;
        Ok(())
    }

    /// Apply a subsequent-symbol S delta (T.88 section 6.4.8).
    fn take_next_s(&mut self, delta: i32, offset: i8) -> Result<()> {
        self.current_s = self
            .current_s
            .checked_add(i64::from(delta))
            .and_then(|value| value.checked_add(i64::from(offset)))
            .ok_or(Jbig2Error::Overflow("text S"))?;
        Ok(())
    }

    /// Count one placed instance.
    fn record(&mut self) -> Result<()> {
        self.placed = self
            .placed
            .checked_add(1)
            .ok_or(Jbig2Error::Overflow("text instances"))?;
        Ok(())
    }
}

/// Scale a strip delta by the strip height (T.88 section 6.4.6).
fn scale_strip_delta(delta: i32, strip_size: u32) -> Result<i64> {
    i64::from(delta)
        .checked_mul(i64::from(strip_size))
        .ok_or(Jbig2Error::Overflow("text strip delta"))
}

/// The symbols a text decoding run may place.
///
/// Only views are exposed, so referring to a dictionary never copies its
/// pixels and globals stay shared across every region that refers to them.
pub(crate) trait SymbolLookup {
    /// Borrow the symbol an identifier selects.
    fn get(&self, id: u32) -> Result<BitmapRef<'_>>;
}

/// Symbols gathered from a text region's referred-to dictionaries.
pub(crate) struct SymbolSet<'a> {
    symbols: Vec<BitmapRef<'a>>,
}

impl<'a> SymbolSet<'a> {
    /// Take ownership of the collected views.
    pub(crate) fn new(symbols: Vec<BitmapRef<'a>>) -> Self {
        Self { symbols }
    }

    /// Number of symbols an identifier may select.
    pub(crate) fn len(&self) -> usize {
        self.symbols.len()
    }
}

impl SymbolLookup for SymbolSet<'_> {
    fn get(&self, id: u32) -> Result<BitmapRef<'_>> {
        let index = usize::try_from(id).map_err(|_| Jbig2Error::Overflow("symbol index"))?;
        self.symbols
            .get(index)
            .copied()
            .ok_or(Jbig2Error::InvalidResource("symbol identifier"))
    }
}

/// Bits a symbol identifier code occupies (T.88 section 6.5.8.2.3).
///
/// A set of one symbol needs no bits at all, and its only identifier decodes
/// as zero.
pub(crate) fn symbol_code_length(count: usize) -> Result<u8> {
    let mut bits = 0_u8;
    let mut capacity = 1_usize;
    while capacity < count {
        bits = bits
            .checked_add(1)
            .ok_or(Jbig2Error::Overflow("symbol code length"))?;
        capacity = capacity
            .checked_mul(2)
            .ok_or(Jbig2Error::Overflow("symbol code length"))?;
    }
    Ok(bits)
}

/// Entropy-specific reads the strip procedure needs from its coding mode.
///
/// The loop owns placement and composition; an implementation supplies only
/// decoded integers and, where enabled, a refined instance bitmap.
pub(crate) trait TextSymbolSource {
    /// Decode `DT` for the next strip (T.88 section 6.4.6).
    fn strip_delta_t(&mut self, budget: &mut DecodeBudget) -> Result<i32>;

    /// Decode `DFS` for a strip's first symbol (T.88 section 6.4.7).
    fn first_delta_s(&mut self, budget: &mut DecodeBudget) -> Result<i32>;

    /// Decode `IDS`, or `None` at the strip's out-of-band terminator
    /// (T.88 section 6.4.8).
    fn next_delta_s(&mut self, budget: &mut DecodeBudget) -> Result<Option<i32>>;

    /// Decode `CURT` within the strip (T.88 section 6.4.5 step 3(c)iii).
    fn current_t(&mut self, budget: &mut DecodeBudget) -> Result<i32>;

    /// Decode the symbol identifier (T.88 section 6.4.5 step 3(c)iv).
    fn symbol_id(&mut self, budget: &mut DecodeBudget) -> Result<u32>;

    /// Decode whether this instance refines its symbol (T.88 section 6.4.11).
    fn refine_flag(&mut self, budget: &mut DecodeBudget) -> Result<bool>;

    /// Decode a refined instance from the selected symbol.
    fn refine(&mut self, reference: BitmapRef<'_>, budget: &mut DecodeBudget) -> Result<Bitmap>;
}

/// Decode and compose every symbol instance of one text decoding run.
///
/// This is the strip procedure of T.88 section 6.4.5 step 3, shared by both
/// text region coding modes and by symbol dictionary aggregation.
pub(crate) fn decode_text(
    layout: TextLayout,
    symbols: &dyn SymbolLookup,
    source: &mut dyn TextSymbolSource,
    target: &mut Bitmap,
    compositor: &mut dyn BitmapCompositor,
    budget: &mut DecodeBudget,
) -> Result<()> {
    budget.charge(LimitKind::Symbols, u64::from(layout.instances))?;
    let mut state = StripState::begin(source.strip_delta_t(budget)?, layout.strip_size)?;
    while state.placed < layout.instances {
        state.begin_strip(source.strip_delta_t(budget)?, layout.strip_size)?;
        state.take_first_s(source.first_delta_s(budget)?)?;
        decode_strip(
            layout, symbols, source, target, compositor, &mut state, budget,
        )?;
    }
    Ok(())
}

/// Decode one strip's instances, up to its out-of-band terminator.
///
/// The terminator always follows the strip's last instance, so it is consumed
/// here rather than left for the next read; a strip that keeps going after the
/// declared instance count is malformed.
fn decode_strip(
    layout: TextLayout,
    symbols: &dyn SymbolLookup,
    source: &mut dyn TextSymbolSource,
    target: &mut Bitmap,
    compositor: &mut dyn BitmapCompositor,
    state: &mut StripState,
    budget: &mut DecodeBudget,
) -> Result<()> {
    loop {
        if state.placed >= layout.instances {
            return Err(Jbig2Error::InvalidResource(
                "text region places more instances than declared",
            ));
        }
        let top = instance_top(state, layout, source, budget)?;
        let id = source.symbol_id(budget)?;
        let symbol = symbols.get(id)?;
        let refined = if source.refine_flag(budget)? {
            Some(source.refine(symbol, budget)?)
        } else {
            None
        };
        let instance = match refined.as_ref() {
            Some(bitmap) => bitmap.as_ref(),
            None => symbol,
        };
        place_instance(state, layout, instance, top, target, compositor, budget)?;
        state.record()?;
        let Some(delta) = source.next_delta_s(budget)? else {
            return Ok(());
        };
        state.take_next_s(delta, layout.delta_s_offset)?;
    }
}

/// Compute an instance's absolute T coordinate.
///
/// A single-row strip encodes no offset, so nothing is read for it.
fn instance_top(
    state: &StripState,
    layout: TextLayout,
    source: &mut dyn TextSymbolSource,
    budget: &mut DecodeBudget,
) -> Result<i64> {
    if layout.strip_size == 1 {
        return Ok(state.strip_top);
    }
    state
        .strip_top
        .checked_add(i64::from(source.current_t(budget)?))
        .ok_or(Jbig2Error::Overflow("text instance T"))
}

/// Place one instance and advance `CURS` past it.
fn place_instance(
    state: &mut StripState,
    layout: TextLayout,
    symbol: BitmapRef<'_>,
    top: i64,
    target: &mut Bitmap,
    compositor: &mut dyn BitmapCompositor,
    budget: &mut DecodeBudget,
) -> Result<()> {
    let (width, height) = (symbol.width(), symbol.height());
    state.current_s = layout.adjust_before(state.current_s, width, height)?;
    let (x, y) = layout.placement(state.current_s, top, width, height)?;
    compositor.compose(
        target,
        symbol,
        Placement {
            x,
            y,
            operator: layout.operator,
        },
        budget,
    )?;
    state.current_s = layout.advance_after(state.current_s, width, height)?;
    Ok(())
}
