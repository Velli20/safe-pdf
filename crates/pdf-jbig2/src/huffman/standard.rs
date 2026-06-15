/// Number of entries in the local standard-table lookup.
///
/// Index zero is intentionally unused so that `STANDARD_TABLE_B1` maps to
/// table B.1 from ITU-T T.88 / ISO/IEC 14492 Annex B.
const STANDARD_TABLE_COUNT_WITH_UNUSED_ZERO: usize = 16;

/// Identifier for one JBIG2 standard Huffman table from Annex B.
///
/// The wrapped value is the Annex B table number. It is intentionally not
/// public so production code must use the named `STANDARD_TABLE_B*` constants
/// instead of unnamed integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StandardTableId(usize);

/// Standard Huffman table B.1 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B1: StandardTableId = StandardTableId(1);
/// Standard Huffman table B.2 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B2: StandardTableId = StandardTableId(2);
/// Standard Huffman table B.3 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B3: StandardTableId = StandardTableId(3);
/// Standard Huffman table B.4 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B4: StandardTableId = StandardTableId(4);
/// Standard Huffman table B.5 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B5: StandardTableId = StandardTableId(5);
/// Standard Huffman table B.6 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B6: StandardTableId = StandardTableId(6);
/// Standard Huffman table B.7 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B7: StandardTableId = StandardTableId(7);
/// Standard Huffman table B.8 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B8: StandardTableId = StandardTableId(8);
/// Standard Huffman table B.9 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B9: StandardTableId = StandardTableId(9);
/// Standard Huffman table B.10 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B10: StandardTableId = StandardTableId(10);
/// Standard Huffman table B.11 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B11: StandardTableId = StandardTableId(11);
/// Standard Huffman table B.12 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B12: StandardTableId = StandardTableId(12);
/// Standard Huffman table B.13 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B13: StandardTableId = StandardTableId(13);
/// Standard Huffman table B.14 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B14: StandardTableId = StandardTableId(14);
/// Standard Huffman table B.15 from ITU-T T.88 / ISO/IEC 14492 Annex B.
pub(crate) const STANDARD_TABLE_B15: StandardTableId = StandardTableId(15);
impl StandardTableId {
    /// Return the local lookup index for this Annex B standard table.
    pub(crate) fn lookup_index(self) -> usize {
        self.0
    }
}

/// One range row in a JBIG2 standard Huffman table.
///
/// ITU-T T.88 / ISO/IEC 14492 Annex B defines rows by prefix length
/// (`PREFLEN`), range length (`RANGELEN`), and range base (`RANGELOW`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HuffmanRangeEntry {
    /// Prefix length (`PREFLEN`) for the row.
    pub(crate) prefix_len: u8,
    /// Number of extra bits (`RANGELEN`) following the prefix.
    pub(crate) range_len: u8,
    /// Base value (`RANGELOW`) for the decoded range.
    pub(crate) range_low: i32,
}

/// Static definition of one standard Huffman table from Annex B.
///
/// `htoob` records whether the final row is the Huffman out-of-band marker
/// defined by ITU-T T.88 / ISO/IEC 14492 Annex B.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StandardTableDef {
    /// Whether this standard table has a Huffman out-of-band marker.
    pub(crate) htoob: bool,
    /// Range rows from the corresponding Annex B table.
    pub(crate) entries: &'static [HuffmanRangeEntry],
}

macro_rules! entry {
    ($prefix_len:expr, $range_len:expr, $range_low:expr) => {
        HuffmanRangeEntry {
            prefix_len: $prefix_len,
            range_len: $range_len,
            range_low: $range_low,
        }
    };
}

/// Standard Huffman tables from ITU-T T.88 / ISO/IEC 14492 Annex B.
///
/// Element zero is unused to keep the array index aligned with the Annex B
/// table number. Production code should use `STANDARD_TABLE_B*` constants.
pub(crate) const STANDARD_TABLES: [StandardTableDef; STANDARD_TABLE_COUNT_WITH_UNUSED_ZERO] = [
    StandardTableDef {
        htoob: false,
        entries: &[],
    },
    StandardTableDef {
        htoob: false,
        entries: &[
            entry!(1, 4, 0),
            entry!(2, 8, 16),
            entry!(3, 16, 272),
            entry!(0, 32, -1),
            entry!(3, 32, 65_808),
        ],
    },
    StandardTableDef {
        htoob: true,
        entries: &[
            entry!(1, 0, 0),
            entry!(2, 0, 1),
            entry!(3, 0, 2),
            entry!(4, 3, 3),
            entry!(5, 6, 11),
            entry!(0, 32, -1),
            entry!(6, 32, 75),
            entry!(6, 0, 0),
        ],
    },
    StandardTableDef {
        htoob: true,
        entries: &[
            entry!(8, 8, -256),
            entry!(1, 0, 0),
            entry!(2, 0, 1),
            entry!(3, 0, 2),
            entry!(4, 3, 3),
            entry!(5, 6, 11),
            entry!(8, 32, -257),
            entry!(7, 32, 75),
            entry!(6, 0, 0),
        ],
    },
    StandardTableDef {
        htoob: false,
        entries: &[
            entry!(1, 0, 1),
            entry!(2, 0, 2),
            entry!(3, 0, 3),
            entry!(4, 3, 4),
            entry!(5, 6, 12),
            entry!(0, 32, -1),
            entry!(5, 32, 76),
        ],
    },
    StandardTableDef {
        htoob: false,
        entries: &[
            entry!(7, 8, -255),
            entry!(1, 0, 1),
            entry!(2, 0, 2),
            entry!(3, 0, 3),
            entry!(4, 3, 4),
            entry!(5, 6, 12),
            entry!(7, 32, -256),
            entry!(6, 32, 76),
        ],
    },
    StandardTableDef {
        htoob: false,
        entries: &[
            entry!(5, 10, -2048),
            entry!(4, 9, -1024),
            entry!(4, 8, -512),
            entry!(4, 7, -256),
            entry!(5, 6, -128),
            entry!(5, 5, -64),
            entry!(4, 5, -32),
            entry!(2, 7, 0),
            entry!(3, 7, 128),
            entry!(3, 8, 256),
            entry!(4, 9, 512),
            entry!(4, 10, 1024),
            entry!(6, 32, -2049),
            entry!(6, 32, 2048),
        ],
    },
    StandardTableDef {
        htoob: false,
        entries: &[
            entry!(4, 9, -1024),
            entry!(3, 8, -512),
            entry!(4, 7, -256),
            entry!(5, 6, -128),
            entry!(5, 5, -64),
            entry!(4, 5, -32),
            entry!(4, 5, 0),
            entry!(5, 5, 32),
            entry!(5, 6, 64),
            entry!(4, 7, 128),
            entry!(3, 8, 256),
            entry!(3, 9, 512),
            entry!(3, 10, 1024),
            entry!(5, 32, -1025),
            entry!(5, 32, 2048),
        ],
    },
    StandardTableDef {
        htoob: true,
        entries: &[
            entry!(8, 3, -15),
            entry!(9, 1, -7),
            entry!(8, 1, -5),
            entry!(9, 0, -3),
            entry!(7, 0, -2),
            entry!(4, 0, -1),
            entry!(2, 1, 0),
            entry!(5, 0, 2),
            entry!(6, 0, 3),
            entry!(3, 4, 4),
            entry!(6, 1, 20),
            entry!(4, 4, 22),
            entry!(4, 5, 38),
            entry!(5, 6, 70),
            entry!(5, 7, 134),
            entry!(6, 7, 262),
            entry!(7, 8, 390),
            entry!(6, 10, 646),
            entry!(9, 32, -16),
            entry!(9, 32, 1670),
            entry!(2, 0, 0),
        ],
    },
    StandardTableDef {
        htoob: true,
        entries: &[
            entry!(8, 4, -31),
            entry!(9, 2, -15),
            entry!(8, 2, -11),
            entry!(9, 1, -7),
            entry!(7, 1, -5),
            entry!(4, 1, -3),
            entry!(3, 1, -1),
            entry!(3, 1, 1),
            entry!(5, 1, 3),
            entry!(6, 1, 5),
            entry!(3, 5, 7),
            entry!(6, 2, 39),
            entry!(4, 5, 43),
            entry!(4, 6, 75),
            entry!(5, 7, 139),
            entry!(5, 8, 267),
            entry!(6, 8, 523),
            entry!(7, 9, 779),
            entry!(6, 11, 1291),
            entry!(9, 32, -32),
            entry!(9, 32, 3339),
            entry!(2, 0, 0),
        ],
    },
    StandardTableDef {
        htoob: true,
        entries: &[
            entry!(7, 4, -21),
            entry!(8, 0, -5),
            entry!(7, 0, -4),
            entry!(5, 0, -3),
            entry!(2, 2, -2),
            entry!(5, 0, 2),
            entry!(6, 0, 3),
            entry!(7, 0, 4),
            entry!(8, 0, 5),
            entry!(2, 6, 6),
            entry!(5, 5, 70),
            entry!(6, 5, 102),
            entry!(6, 6, 134),
            entry!(6, 7, 198),
            entry!(6, 8, 326),
            entry!(6, 9, 582),
            entry!(6, 10, 1094),
            entry!(7, 11, 2118),
            entry!(8, 32, -22),
            entry!(8, 32, 4166),
            entry!(2, 0, 0),
        ],
    },
    StandardTableDef {
        htoob: false,
        entries: &[
            entry!(1, 0, 1),
            entry!(2, 1, 2),
            entry!(4, 0, 4),
            entry!(4, 1, 5),
            entry!(5, 1, 7),
            entry!(5, 2, 9),
            entry!(6, 2, 13),
            entry!(7, 2, 17),
            entry!(7, 3, 21),
            entry!(7, 4, 29),
            entry!(7, 5, 45),
            entry!(7, 6, 77),
            entry!(0, 32, 0),
            entry!(7, 32, 141),
        ],
    },
    StandardTableDef {
        htoob: false,
        entries: &[
            entry!(1, 0, 1),
            entry!(2, 0, 2),
            entry!(3, 1, 3),
            entry!(5, 0, 5),
            entry!(5, 1, 6),
            entry!(6, 1, 8),
            entry!(7, 0, 10),
            entry!(7, 1, 11),
            entry!(7, 2, 13),
            entry!(7, 3, 17),
            entry!(7, 4, 25),
            entry!(8, 5, 41),
            entry!(0, 32, 0),
            entry!(8, 32, 73),
        ],
    },
    StandardTableDef {
        htoob: false,
        entries: &[
            entry!(1, 0, 1),
            entry!(3, 0, 2),
            entry!(4, 0, 3),
            entry!(5, 0, 4),
            entry!(4, 1, 5),
            entry!(3, 3, 7),
            entry!(6, 1, 15),
            entry!(6, 2, 17),
            entry!(6, 3, 21),
            entry!(6, 4, 29),
            entry!(6, 5, 45),
            entry!(7, 6, 77),
            entry!(0, 32, 0),
            entry!(7, 32, 141),
        ],
    },
    StandardTableDef {
        htoob: false,
        entries: &[
            entry!(3, 0, -2),
            entry!(3, 0, -1),
            entry!(1, 0, 0),
            entry!(3, 0, 1),
            entry!(3, 0, 2),
            entry!(0, 32, -3),
            entry!(0, 32, 3),
        ],
    },
    StandardTableDef {
        htoob: false,
        entries: &[
            entry!(7, 4, -24),
            entry!(6, 2, -8),
            entry!(5, 1, -4),
            entry!(4, 0, -2),
            entry!(3, 0, -1),
            entry!(1, 0, 0),
            entry!(3, 0, 1),
            entry!(4, 0, 2),
            entry!(5, 1, 3),
            entry!(6, 2, 5),
            entry!(7, 4, 9),
            entry!(7, 32, -25),
            entry!(7, 32, 25),
        ],
    },
];
