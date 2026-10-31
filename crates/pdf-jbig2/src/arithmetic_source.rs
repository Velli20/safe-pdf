//! Arithmetic entropy source for symbol dictionaries and text regions.
//!
//! One arithmetic decoder and one set of context banks serve a whole segment:
//! generic bitmap contexts, refinement contexts, and every integer procedure
//! persist across symbols, height classes, and strips. A symbol dictionary
//! drives this through [`SymbolSource`] and a text region through
//! [`TextSymbolSource`]; dictionary aggregation uses both at once, which is why
//! one type implements them together.

use crate::{
    Jbig2Error, Result,
    arithmetic::{
        ArithmeticContexts, ArithmeticDecoder, IntegerContext, IntegerContexts, IntegerValue,
    },
    bitmap::{Bitmap, BitmapRef, CombinationOperator, DefaultCompositor},
    dictionary::{RetainedContexts, SymbolDictionaryDescriptor},
    document::DecodeBudget,
    error::SourceLocation,
    region::{GenericPlane, RefinementProcedure},
    symbols::{DictionarySymbols, SymbolOutcome, SymbolSize, SymbolSource},
    template::{GenericTemplate, RefinementTemplate},
    text::{SymbolLookup, TextLayout, TextSymbolSource, decode_text, symbol_code_length},
};

/// Arithmetic decoding state shared by one segment's whole coded payload.
pub(crate) struct ArithmeticSource<'a> {
    decoder: ArithmeticDecoder<'a>,
    integers: IntegerContexts,
    generic: ArithmeticContexts,
    refinement: ArithmeticContexts,
    /// Present only where the segment codes symbol bitmaps directly.
    generic_template: Option<GenericTemplate>,
    /// Present only where refinement or aggregation is enabled, which for a
    /// dictionary means every new symbol is refined or aggregated.
    refinement_template: Option<RefinementTemplate>,
    code_bits: u8,
}

impl<'a> ArithmeticSource<'a> {
    /// Prepare decoding for a symbol dictionary segment.
    ///
    /// `symbol_count` is the imported plus newly declared symbol total, which
    /// fixes the identifier code width for the whole dictionary.
    pub(crate) fn for_dictionary(
        descriptor: &SymbolDictionaryDescriptor<'a>,
        symbol_count: usize,
        inherited: RetainedContexts,
        location: SourceLocation,
        budget: &mut DecodeBudget,
    ) -> Result<Self> {
        let generic_template = descriptor
            .generic_template
            .ok_or(Jbig2Error::InvalidResource("symbol dictionary template"))?;
        Self::new(
            descriptor.data,
            Some(generic_template),
            descriptor.refinement_template,
            inherited,
            symbol_count,
            location,
            budget,
        )
    }

    /// Prepare decoding for a text region segment, which codes no new symbols.
    pub(crate) fn for_text(
        data: &'a [u8],
        refinement_template: Option<RefinementTemplate>,
        symbol_count: usize,
        location: SourceLocation,
        budget: &mut DecodeBudget,
    ) -> Result<Self> {
        Self::new(
            data,
            None,
            refinement_template,
            RetainedContexts::default(),
            symbol_count,
            location,
            budget,
        )
    }

    /// Allocate every context bank this segment's procedures will address.
    fn new(
        data: &'a [u8],
        generic_template: Option<GenericTemplate>,
        refinement_template: Option<RefinementTemplate>,
        inherited: RetainedContexts,
        symbol_count: usize,
        location: SourceLocation,
        budget: &mut DecodeBudget,
    ) -> Result<Self> {
        let generic_contexts = match generic_template {
            Some(template) => template.id().context_count(),
            None => 0,
        };
        let refinement_contexts = match refinement_template {
            Some(template) => template.id().context_count(),
            None => 0,
        };
        let code_bits = symbol_code_length(symbol_count)?;
        Ok(Self {
            decoder: ArithmeticDecoder::new(data, location)?,
            integers: IntegerContexts::new(code_bits, budget)?,
            generic: ArithmeticContexts::inherit(inherited.generic, generic_contexts, budget)?,
            refinement: ArithmeticContexts::inherit(
                inherited.refinement,
                refinement_contexts,
                budget,
            )?,
            generic_template,
            refinement_template,
            code_bits,
        })
    }

    /// Decode one integer field, rejecting an out-of-band result.
    fn integer(
        &mut self,
        which: IntegerContext,
        what: &'static str,
        budget: &mut DecodeBudget,
    ) -> Result<i32> {
        self.optional_integer(which, budget)?.coordinate(what)
    }

    /// Decode one integer field that may terminate with an out-of-band code.
    fn optional_integer(
        &mut self,
        which: IntegerContext,
        budget: &mut DecodeBudget,
    ) -> Result<IntegerValue> {
        let contexts = self.integers.procedure(which)?;
        self.decoder.decode_integer(contexts, budget)
    }

    /// Decode a symbol coded directly as a generic bitmap (T.88 section 6.5.8.1).
    fn decode_generic(&mut self, size: SymbolSize, budget: &mut DecodeBudget) -> Result<Bitmap> {
        let template = self
            .generic_template
            .ok_or(Jbig2Error::InvalidResource("symbol dictionary template"))?;
        let plane = GenericPlane {
            width: size.width,
            height: size.height,
            template,
            typical_prediction: false,
            skip: None,
        };
        plane.decode(&mut self.decoder, &mut self.generic, budget)
    }

    /// Decode a refined or aggregated symbol (T.88 section 6.5.8.2).
    fn decode_refined(
        &mut self,
        size: SymbolSize,
        symbols: DictionarySymbols<'_>,
        budget: &mut DecodeBudget,
    ) -> Result<Bitmap> {
        let instances = self.integer(
            IntegerContext::AggregateCount,
            "aggregate instances",
            budget,
        )?;
        let instances =
            u32::try_from(instances).map_err(|_| Jbig2Error::InvalidResource("aggregate count"))?;
        if instances == 1 {
            return self.decode_single_refinement(size, symbols, budget);
        }
        self.decode_aggregate(size, symbols, instances, budget)
    }

    /// Refine one referenced symbol into this symbol's dimensions.
    fn decode_single_refinement(
        &mut self,
        size: SymbolSize,
        symbols: DictionarySymbols<'_>,
        budget: &mut DecodeBudget,
    ) -> Result<Bitmap> {
        let id = self.decode_identifier(budget)?;
        let reference_dx = self.integer(IntegerContext::RefineX, "refinement X", budget)?;
        let reference_dy = self.integer(IntegerContext::RefineY, "refinement Y", budget)?;
        let reference = symbols.get(id)?;
        self.refine_into(size, reference, reference_dx, reference_dy, budget)
    }

    /// Compose several symbol instances into one new symbol
    /// (T.88 section 6.5.8.2.1), which is the text region procedure in miniature.
    fn decode_aggregate(
        &mut self,
        size: SymbolSize,
        symbols: DictionarySymbols<'_>,
        instances: u32,
        budget: &mut DecodeBudget,
    ) -> Result<Bitmap> {
        let mut bitmap = Bitmap::new(size.width, size.height, false, budget)?;
        let layout = TextLayout {
            instances,
            strip_size: 1,
            reference_corner: crate::region::ReferenceCorner::TopLeft,
            transposed: false,
            operator: CombinationOperator::Or,
            delta_s_offset: 0,
        };
        // Aggregation assembles one symbol's own pixels rather than page
        // output, so it always uses the scalar compositor with the OR operator
        // that T.88 section 6.5.8.2.1 fixes for this procedure.
        let mut compositor = DefaultCompositor;
        decode_text(layout, &symbols, self, &mut bitmap, &mut compositor, budget)?;
        Ok(bitmap)
    }

    /// Run the refinement procedure with this dictionary's template.
    fn refine_into(
        &mut self,
        size: SymbolSize,
        reference: BitmapRef<'_>,
        reference_dx: i32,
        reference_dy: i32,
        budget: &mut DecodeBudget,
    ) -> Result<Bitmap> {
        let template = self
            .refinement_template
            .ok_or(Jbig2Error::InvalidResource("refinement template"))?;
        let procedure = RefinementProcedure {
            width: size.width,
            height: size.height,
            template,
            typical_prediction: false,
            reference_dx,
            reference_dy,
        };
        procedure.decode(reference, &mut self.decoder, &mut self.refinement, budget)
    }

    /// Decode one symbol identifier.
    fn decode_identifier(&mut self, budget: &mut DecodeBudget) -> Result<u32> {
        let bits = self.code_bits;
        self.decoder
            .decode_symbol_id(self.integers.identifier(), bits, budget)
    }
}

impl SymbolSource for ArithmeticSource<'_> {
    fn delta_height(&mut self, budget: &mut DecodeBudget) -> Result<i32> {
        self.integer(IntegerContext::DeltaHeight, "symbol height delta", budget)
    }

    fn delta_width(&mut self, budget: &mut DecodeBudget) -> Result<Option<i32>> {
        match self.optional_integer(IntegerContext::DeltaWidth, budget)? {
            IntegerValue::OutOfBand => Ok(None),
            value => value.coordinate("symbol width delta").map(Some),
        }
    }

    fn decode_symbol(
        &mut self,
        size: SymbolSize,
        symbols: DictionarySymbols<'_>,
        budget: &mut DecodeBudget,
    ) -> Result<SymbolOutcome> {
        if self.refinement_template.is_some() {
            return self
                .decode_refined(size, symbols, budget)
                .map(SymbolOutcome::Ready);
        }
        self.decode_generic(size, budget).map(SymbolOutcome::Ready)
    }

    fn export_run(&mut self, budget: &mut DecodeBudget) -> Result<u32> {
        let run = self.integer(IntegerContext::ExportRun, "export run", budget)?;
        u32::try_from(run).map_err(|_| Jbig2Error::InvalidResource("negative export run"))
    }

    fn into_contexts(self: Box<Self>) -> RetainedContexts {
        RetainedContexts {
            generic: Some(self.generic),
            refinement: Some(self.refinement),
        }
    }
}

impl TextSymbolSource for ArithmeticSource<'_> {
    fn strip_delta_t(&mut self, budget: &mut DecodeBudget) -> Result<i32> {
        self.integer(IntegerContext::DeltaT, "aggregate strip delta", budget)
    }

    fn first_delta_s(&mut self, budget: &mut DecodeBudget) -> Result<i32> {
        self.integer(IntegerContext::FirstS, "aggregate first S", budget)
    }

    fn next_delta_s(&mut self, budget: &mut DecodeBudget) -> Result<Option<i32>> {
        match self.optional_integer(IntegerContext::DeltaS, budget)? {
            IntegerValue::OutOfBand => Ok(None),
            value => value.coordinate("aggregate S delta").map(Some),
        }
    }

    fn current_t(&mut self, budget: &mut DecodeBudget) -> Result<i32> {
        self.integer(IntegerContext::StripT, "aggregate strip T", budget)
    }

    fn symbol_id(&mut self, budget: &mut DecodeBudget) -> Result<u32> {
        self.decode_identifier(budget)
    }

    fn refine_flag(&mut self, budget: &mut DecodeBudget) -> Result<bool> {
        // `RI` is coded only when instance refinement is enabled
        // (T.88 section 6.4.11); otherwise every instance is used as is.
        if self.refinement_template.is_none() {
            return Ok(false);
        }
        Ok(self.integer(
            IntegerContext::RefineFlag,
            "aggregate refinement flag",
            budget,
        )? != 0)
    }

    fn refine(&mut self, reference: BitmapRef<'_>, budget: &mut DecodeBudget) -> Result<Bitmap> {
        let delta_width = self.integer(IntegerContext::RefineWidth, "refinement width", budget)?;
        let delta_height =
            self.integer(IntegerContext::RefineHeight, "refinement height", budget)?;
        let delta_x = self.integer(IntegerContext::RefineX, "refinement X", budget)?;
        let delta_y = self.integer(IntegerContext::RefineY, "refinement Y", budget)?;
        let size = refined_size(reference, delta_width, delta_height)?;
        let reference_dx = centered_offset(delta_width, delta_x)?;
        let reference_dy = centered_offset(delta_height, delta_y)?;
        self.refine_into(size, reference, reference_dx, reference_dy, budget)
    }
}

impl SymbolLookup for DictionarySymbols<'_> {
    fn get(&self, id: u32) -> Result<BitmapRef<'_>> {
        DictionarySymbols::get(*self, id)
    }
}

/// Apply signed size deltas to a reference symbol (T.88 section 6.4.11.1).
pub(crate) fn refined_size(
    reference: BitmapRef<'_>,
    delta_width: i32,
    delta_height: i32,
) -> Result<SymbolSize> {
    Ok(SymbolSize {
        width: apply_delta(reference.width(), delta_width, "refined symbol width")?,
        height: apply_delta(reference.height(), delta_height, "refined symbol height")?,
    })
}

/// Apply one signed size delta, rejecting a negative result.
fn apply_delta(base: u32, delta: i32, what: &'static str) -> Result<u32> {
    let value = i64::from(base)
        .checked_add(i64::from(delta))
        .ok_or(Jbig2Error::Overflow(what))?;
    u32::try_from(value).map_err(|_| Jbig2Error::InvalidResource(what))
}

/// Centre a refined instance over its reference (T.88 section 6.4.11 step 3(c)xi).
pub(crate) fn centered_offset(size_delta: i32, delta: i32) -> Result<i32> {
    size_delta
        .div_euclid(2)
        .checked_add(delta)
        .ok_or(Jbig2Error::Overflow("refinement displacement"))
}
