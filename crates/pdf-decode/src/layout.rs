//! Layout information for packed PDF sample data.

/// Describes how packed sample codes are arranged in the input byte stream.
#[derive(Debug, Clone, Copy)]
pub enum SampleLayout {
    /// Treat the input as a single contiguous sample sequence.
    Contiguous {
        /// The total number of sample codes to read.
        sample_count: usize,
    },
    /// Treat the input as row-aligned image data with per-row padding.
    RowAligned {
        /// The number of pixels in each row.
        width: usize,
        /// The number of rows in the image.
        height: usize,
        /// The number of sample components per pixel.
        samples_per_pixel: usize,
    },
}
