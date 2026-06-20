use crate::{error::Jbig2Error, huffman::HuffmanCode};
use pdf_utils::BitReader;

/// A child edge in the binary prefix-code decode tree.
///
/// ITU-T T.88 / ISO/IEC 14492 Annex B decodes Huffman prefixes bit by bit;
/// this enum records whether a bit path is still empty, continues to another
/// internal node, or resolves to a range-table symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    Empty,
    Node(usize),
    Leaf(usize),
}

impl Default for Link {
    fn default() -> Self {
        Self::Empty
    }
}

/// A node in the binary prefix-code decode tree.
///
/// The `zero` and `one` fields are the next links for the next decoded bit in
/// the Annex B Huffman prefix.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DecodeNode {
    zero: Link,
    one: Link,
}

/// Prefix-code lookup tree for canonical JBIG2 Huffman codes.
///
/// The tree is built from the canonical codes assigned by ITU-T T.88 /
/// ISO/IEC 14492 Annex B and returns the table-entry index associated with
/// the decoded prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecodeTree {
    nodes: Vec<DecodeNode>,
}

impl DecodeTree {
    /// Build a decode tree from canonical Huffman codes.
    ///
    /// Zero-length entries from Annex B tables do not have prefixes and are
    /// skipped. Conflicting prefixes are reported as invalid Huffman tables.
    pub(crate) fn new(codes: &[HuffmanCode]) -> Result<Self, Jbig2Error> {
        let mut tree = Self {
            nodes: vec![DecodeNode::default()],
        };
        for (symbol, code) in codes.iter().enumerate() {
            if code.codelen != 0 {
                tree.insert(*code, symbol)?;
            }
        }
        Ok(tree)
    }

    /// Decode one Huffman symbol index from `reader`.
    ///
    /// This is the prefix traversal portion of the ITU-T T.88 / ISO/IEC 14492
    /// Annex B Huffman Table Decoding Procedure. The caller supplies
    /// `stream_name` for truncation diagnostics.
    pub(crate) fn decode(
        &self,
        reader: &mut BitReader<'_>,
        stream_name: &'static str,
    ) -> Result<usize, Jbig2Error> {
        let mut node_index = 0usize;
        loop {
            let bit = reader
                .next_bit()
                .ok_or(Jbig2Error::Truncated(stream_name))?;
            let node = self
                .nodes
                .get(node_index)
                .ok_or(Jbig2Error::InvalidTable("Huffman decode tree"))?;
            let link = if bit { node.one } else { node.zero };
            match link {
                Link::Empty => return Err(Jbig2Error::InvalidTable(stream_name)),
                Link::Node(next) => node_index = next,
                Link::Leaf(symbol) => return Ok(symbol),
            }
        }
    }

    /// Insert one canonical prefix into the tree.
    ///
    /// Used while constructing the Annex B decode tree; `symbol` is the
    /// corresponding range-table entry index.
    fn insert(&mut self, code: HuffmanCode, symbol: usize) -> Result<(), Jbig2Error> {
        let mut node_index = 0usize;
        for bit_index in (0..code.codelen).rev() {
            let bit = code
                .code
                .checked_shr(u32::from(bit_index))
                .ok_or(Jbig2Error::InvalidTable("Huffman code"))?
                & 1;
            let is_leaf = bit_index == 0;
            if is_leaf {
                self.insert_leaf(node_index, bit, symbol)?;
            } else {
                node_index = self.insert_node(node_index, bit)?;
            }
        }
        Ok(())
    }

    /// Insert or follow an internal branch node for a non-final prefix bit.
    ///
    /// A leaf encountered before the final bit means two Annex B prefixes
    /// overlap and the table is invalid.
    fn insert_node(&mut self, node_index: usize, bit: u32) -> Result<usize, Jbig2Error> {
        let new_index = self.nodes.len();
        let (next, create) = {
            let node = self
                .nodes
                .get_mut(node_index)
                .ok_or(Jbig2Error::InvalidTable("Huffman decode tree"))?;
            let child = if bit == 0 {
                &mut node.zero
            } else {
                &mut node.one
            };
            match *child {
                Link::Empty => {
                    *child = Link::Node(new_index);
                    (new_index, true)
                }
                Link::Node(next) => (next, false),
                Link::Leaf(_) => return Err(Jbig2Error::InvalidTable("Huffman code")),
            }
        };
        if create {
            self.nodes.push(DecodeNode::default());
        }
        Ok(next)
    }

    /// Insert the leaf for the final prefix bit.
    ///
    /// Existing nodes or leaves at this position indicate a duplicate or
    /// prefix-overlapping Huffman code.
    fn insert_leaf(
        &mut self,
        node_index: usize,
        bit: u32,
        symbol: usize,
    ) -> Result<(), Jbig2Error> {
        let node = self
            .nodes
            .get_mut(node_index)
            .ok_or(Jbig2Error::InvalidTable("Huffman decode tree"))?;
        let child = if bit == 0 {
            &mut node.zero
        } else {
            &mut node.one
        };
        match *child {
            Link::Empty => {
                *child = Link::Leaf(symbol);
                Ok(())
            }
            Link::Node(_) | Link::Leaf(_) => Err(Jbig2Error::InvalidTable("Huffman code")),
        }
    }
}
