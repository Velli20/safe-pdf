//! Coefficients for converting uncalibrated DeviceCMYK colors to sRGB.

/// Polynomial coefficients that approximate the conventional conversion of
/// uncalibrated DeviceCMYK to sRGB.
///
/// # Layout
///
/// There is one row per sRGB output channel (red, green, blue). Each row
/// weights the 69 monomials of degree one to four in `c`, `m`, `y` and `k`, in
/// the order [`Color::from_cmyk`](crate::color::Color::from_cmyk) lists them.
/// Each component is in `0.0..=1.0`. The weighted sum is an offset on the
/// 0..=255 scale, added to 255 and then clamped. The polynomial has no constant
/// term, so CMYK `0 0 0 0` maps to exactly white.
///
/// # What is approximated
///
/// A DeviceCMYK color with no output intent or default CMYK color space has no
/// defined colorimetry. In practice, viewers convert it with the Adobe-style
/// press conversion: a CMYK to sRGB lookup grid with 9 nodes per axis,
/// interpolated separately along each axis, with the components first
/// rounded to 8 bits. Matching that conversion makes CMYK artwork look as it
/// does elsewhere. Saturated reds come out as 238, 29, 35, not as an orange
/// that clips at 255. Grays carry the slightly warm cast of real press ink.
/// The naive `(1 - c) * (1 - k)` inversion renders everything too bright and
/// saturated.
///
/// # How the coefficients were derived
///
/// The coefficients were fitted offline, not computed at build time:
///
/// 1. The reference conversion was evaluated on a grid of 17 values per axis:
///    13 even steps over 0..=255, merged with the 9 nodes of the reference
///    grid. That gives about 83,000 CMYK samples, which include interpolated
///    points between nodes.
/// 2. For each output channel separately, a least-squares fit solved for the
///    offset from 255 over all monomials of degree one to four.
/// 3. Reference outputs that are clipped at 0 or 255 only bound the true
///    curve; they don't pin it to a value. These samples were treated as
///    censored. Wherever the polynomial already lay beyond the clip, the target
///    was replaced by the polynomial's own value, and the fit was repeated
///    until it settled. Without this, the flat clipped regions pull the
///    polynomial away from saturated colors.
/// 4. The 16 corners of the CMYK cube (paper white, the inks, their overprints
///    and black) were weighted 30 times more than other samples. Spot colors
///    and solid fills are most often exactly these values.
///
/// # Accuracy and trade-offs
///
/// - **Accuracy.** On random 8-bit CMYK colors the result is within 1.8 units
///   RMS of the reference, and 96.5% of colors are within 5 units. Every cube
///   corner is within 2 units. The largest errors, up to about 27 units, are in
///   the red channel around 75% cyan. There the reference grid drops steeply
///   between two nodes, and a smooth polynomial can't follow the kink.
/// - **Why degree four.** Lower degrees miss the saturated corners by tens of
///   units. A second-order fit renders 100% magenta plus yellow as clipped
///   orange, and a third-order fit is still up to 13 units off on several
///   corners.
/// - **Why a polynomial instead of the grid.** A polynomial is compact and
///   smooth, needs no lookup data, and costs one fixed set of multiply-adds per
///   color. It handles fill colors and CMYK image samples alike.
pub(crate) const CMYK_TO_SRGB: [[f32; 69]; 3] = [
    [
        -289.820_28,
        -26.405_91,
        16.878_933,
        -240.811_43,
        101.486_39,
        118.238_35,
        49.033_92,
        406.911_5,
        7.670_523_6,
        -5.253_720_3,
        18.341_305,
        -27.297_83,
        -12.669_027,
        5.334_434,
        -261.287_3,
        150.185_26,
        19.665_37,
        -234.836_17,
        -88.129_08,
        -67.233_665,
        -7.659_746_6,
        -12.011_507,
        -34.665_596,
        -397.317_38,
        0.839_436_77,
        9.872_704,
        -18.031_008,
        -5.950_904_4,
        -9.515_92,
        54.705_96,
        30.035_564,
        13.009_088_5,
        -9.221_388,
        179.333_72,
        103.318_19,
        -97.058_74,
        -41.049_023,
        177.939_54,
        96.070_595,
        35.315_777,
        -167.201_26,
        24.607_466,
        -5.027_580_3,
        174.133_56,
        -25.018_223,
        -3.397_590_6,
        45.627_144,
        2.010_646_8,
        27.007_929,
        -22.890_99,
        -0.165_403_6,
        -13.259_176,
        19.876_163,
        178.501_02,
        0.572_484_14,
        -1.853_848_1,
        8.842_941,
        -3.144_034_4,
        5.741_635,
        -13.823_163,
        2.180_285,
        8.151_793_5,
        11.275_144,
        -46.203_05,
        -13.827_385,
        -1.457_942_8,
        -3.803_304_2,
        -1.211_836_6,
        -163.98,
    ],
    [
        -101.353_76,
        -245.319_26,
        -10.016_459,
        -247.565_43,
        17.893_38,
        127.305_855,
        1.650_924_9,
        101.658_81,
        111.980_4,
        33.673_607,
        379.950_07,
        -10.800_023,
        -5.952_939_5,
        81.169_05,
        0.338_755_2,
        -31.308_949,
        4.843_125_3,
        -11.602_121,
        -64.722_43,
        -19.155_277,
        -136.113_45,
        7.195_91,
        20.557_814,
        -36.149_208,
        -53.529_305,
        -15.922_503,
        -220.790_2,
        -13.486_365,
        -23.472_912,
        -395.339_84,
        10.174_108,
        6.798_382_8,
        16.391_426,
        41.523_93,
        2.429_068,
        4.791_363,
        -4.241_815,
        -6.834_973,
        -19.626_665,
        7.791_645,
        49.451_44,
        -2.759_317,
        -1.552_442_4,
        -2.399_538_8,
        108.433_05,
        -23.225_058,
        -51.276_85,
        9.884_429,
        17.835_203,
        53.041_084,
        -1.890_346_2,
        -11.930_101,
        -8.408_125,
        9.632_79,
        -64.629_85,
        37.296_772,
        102.943_03,
        -3.887_802_6,
        -16.903_088,
        138.213_78,
        0.936_231_43,
        20.659_605,
        -2.551_016,
        185.214_51,
        -2.721_360_2,
        -4.029_230_6,
        2.499_114_8,
        -6.960_671_4,
        -99.279_51,
    ],
    [
        -14.995_909,
        -127.974_24,
        -230.537_49,
        -237.921_46,
        -4.971_067,
        3.081_464_5,
        58.990_795,
        -7.576_079_4,
        10.312_547,
        149.781_59,
        146.469_5,
        112.305_374,
        297.389_25,
        81.725_27,
        6.238_456,
        24.979_956,
        -46.707_607,
        7.058_919_4,
        18.420_967,
        -63.741_69,
        10.015_482,
        38.206_356,
        -51.781_68,
        47.473_785,
        0.486_661_2,
        -52.906_8,
        -26.304_728,
        -5.097_891_3,
        -188.347_93,
        -107.482_09,
        -185.500_03,
        -71.386_284,
        -294.107_97,
        13.445_717,
        -1.534_453_5,
        -13.057_219_5,
        16.830_952,
        -4.404_571,
        -6.313_563_3,
        33.493_282,
        -12.295_006,
        -32.577_244,
        40.221_046,
        -1.968_186,
        -5.386_291,
        19.152_802,
        -9.496_637,
        -63.847_31,
        62.003_067,
        -14.556_384,
        55.998_184,
        -44.380_28,
        -16.282_48,
        -21.126_434,
        2.100_415,
        9.981_647_5,
        -3.496_080_6,
        2.838_337_7,
        30.649_479,
        31.534_952,
        41.969_71,
        -11.069_034,
        70.387_436,
        42.551_09,
        52.032_032,
        36.421_16,
        65.395_79,
        155.502_04,
        -80.389_435,
    ],
];
