use crate::{
    error::Jbig2Error,
    generic_region::{GenericRegionAdaptiveTemplate, GenericRegionTemplate},
};

const NORMALIZED_GBAT_LEN: usize = 8;
const PATTERN_REFERENCE_X_INDEX: usize = 0;
const PATTERN_REFERENCE_Y_INDEX: usize = 1;
const TEMPLATE0_GBAT: [i8; NORMALIZED_GBAT_LEN] = [0, 0, -3, -1, 2, -2, -2, -2];
const TEMPLATE123_GBAT: [i8; NORMALIZED_GBAT_LEN] = [0, 0, 0, 0, 0, 0, 0, 0];

/// Build pattern-dictionary adaptive template data for generic-region decoding.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.4 decodes the pattern dictionary
/// collective bitmap through the generic-region arithmetic procedure. Section
/// 6.2.5.4 defines the adaptive-template coordinate table consumed by that
/// procedure.
pub(crate) fn pattern_dictionary_template(
    pattern_width: u8,
    template: GenericRegionTemplate,
) -> Result<GenericRegionAdaptiveTemplate, Jbig2Error> {
    let reference_x = collective_reference_x(pattern_width)?;
    let normalized = normalized_template(reference_x, template);
    Ok(GenericRegionAdaptiveTemplate::from_normalized(normalized))
}

/// Return the negative X offset used to reference the prior pattern cell.
///
/// T.88 section 7.4.4 lays out pattern dictionary cells side by side in the
/// collective bitmap. The first adaptive pixel is placed one pattern width to
/// the left so arithmetic decoding can condition on the previous cell.
fn collective_reference_x(pattern_width: u8) -> Result<i8, Jbig2Error> {
    let pattern_width = i8::try_from(pattern_width)
        .map_err(|_| Jbig2Error::Overflow("pattern width to i8 conversion overflow"))?;
    pattern_width
        .checked_neg()
        .ok_or(Jbig2Error::Overflow("pattern width negation overflow"))
}

/// Return the normalized `GBAT` table used for a pattern dictionary bitmap.
///
/// Generic-region template 0 keeps the remaining adaptive pixels from the
/// section 6.2.5.4 template-0 shape. Templates 1 through 3 only need the
/// prior-pattern reference pair for the collective bitmap procedure.
fn normalized_template(
    reference_x: i8,
    template: GenericRegionTemplate,
) -> [i8; NORMALIZED_GBAT_LEN] {
    let mut normalized = match template {
        GenericRegionTemplate::Template0 => TEMPLATE0_GBAT,
        GenericRegionTemplate::Template1
        | GenericRegionTemplate::Template2
        | GenericRegionTemplate::Template3 => TEMPLATE123_GBAT,
    };
    if let Some(x) = normalized.get_mut(PATTERN_REFERENCE_X_INDEX) {
        *x = reference_x;
    }
    if let Some(y) = normalized.get_mut(PATTERN_REFERENCE_Y_INDEX) {
        *y = 0;
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::pattern_dictionary_template;
    use crate::{error::Jbig2Error, generic_region::GenericRegionTemplate};

    #[test]
    fn template0_uses_prior_pattern_reference_and_template0_shape() {
        let template =
            pattern_dictionary_template(3, GenericRegionTemplate::Template0).expect("template");

        assert_eq!(template.normalized(), [-3, 0, -3, -1, 2, -2, -2, -2]);
    }

    #[test]
    fn templates_one_two_and_three_use_only_prior_pattern_reference() {
        for template_id in [
            GenericRegionTemplate::Template1,
            GenericRegionTemplate::Template2,
            GenericRegionTemplate::Template3,
        ] {
            let template = pattern_dictionary_template(3, template_id).expect("template");

            assert_eq!(template.normalized(), [-3, 0, 0, 0, 0, 0, 0, 0]);
        }
    }

    #[test]
    fn pattern_width_must_fit_signed_adaptive_offset() {
        assert_eq!(
            pattern_dictionary_template(128, GenericRegionTemplate::Template0)
                .expect_err("overflow"),
            Jbig2Error::Overflow("pattern width to i8 conversion overflow")
        );
    }
}
