use std::{ops::Deref, sync::Arc};

/// Storage for a parsed or synthesized font program.
#[derive(Debug, Clone)]
pub enum FontData {
    /// Bundled font bytes with static storage duration.
    Static(&'static [u8]),
    /// Font bytes shared with a decoded PDF stream.
    Shared(SharedFontData),
}

/// A view over the leading bytes of a shared decoded stream.
#[derive(Debug, Clone)]
pub struct SharedFontData {
    data: Arc<Vec<u8>>,
    visible_len: usize,
}

impl FontData {
    /// Shares an entire decoded stream allocation.
    pub fn shared(data: Arc<Vec<u8>>) -> Self {
        let visible_len = data.len();
        Self::Shared(SharedFontData { data, visible_len })
    }

    /// Shares a prefix of a decoded stream allocation.
    ///
    /// `visible_len` is clamped to the available stream length.
    pub fn shared_prefix(data: Arc<Vec<u8>>, visible_len: usize) -> Self {
        let visible_len = visible_len.min(data.len());
        Self::Shared(SharedFontData { data, visible_len })
    }
}

impl Deref for FontData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Static(data) => data,
            Self::Shared(data) => data.data.get(..data.visible_len).unwrap_or_default(),
        }
    }
}

impl AsRef<[u8]> for FontData {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl From<Vec<u8>> for FontData {
    fn from(data: Vec<u8>) -> Self {
        Self::shared(Arc::new(data))
    }
}

impl From<&'static [u8]> for FontData {
    fn from(data: &'static [u8]) -> Self {
        Self::Static(data)
    }
}

impl From<Arc<Vec<u8>>> for FontData {
    fn from(data: Arc<Vec<u8>>) -> Self {
        Self::shared(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_prefix_reuses_allocation_and_clamps_length() {
        let data = Arc::new(vec![1, 2, 3, 4]);
        let original = data.as_ptr();

        let prefix = FontData::shared_prefix(Arc::clone(&data), 2);
        assert_eq!(prefix.as_ptr(), original);
        assert_eq!(prefix.as_ref(), [1, 2]);

        let full = FontData::shared_prefix(data, usize::MAX);
        assert_eq!(full.as_ref(), [1, 2, 3, 4]);
    }

    #[test]
    fn vec_conversion_reuses_allocation() {
        let data = vec![1, 2, 3, 4];
        let original = data.as_ptr();

        let font_data = FontData::from(data);

        assert!(matches!(&font_data, FontData::Shared(_)));
        assert_eq!(font_data.as_ptr(), original);
    }
}
