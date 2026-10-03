//! Tag trees, the quad-tree code used by packet headers.
//!
//! Annex B.10.2 encodes two per-precinct arrays as tag trees: the layer in
//! which each code-block first contributes, and each code-block's number of
//! missing most significant bit-planes. A tag tree is queried against a
//! threshold and consumes only the bits needed to decide whether the leaf's
//! value is below it, so the same tree is refined across successive layers.

use pdf_graphics::Size;

use crate::{
    Jpeg2000Error,
    stuffed_bits::{HeaderSource, StuffedBitReader},
    workspace::Workspace,
};

/// Levels a tag tree can hold, one per halving of the larger axis.
const MAX_LEVELS: usize = 32;
/// Node value meaning "not yet resolved below any threshold".
const UNDETERMINED: u32 = u32::MAX;

/// One tag-tree node: its resolved value and the lower bound proved so far.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TagNode {
    value: u32,
    low: u32,
}

impl Default for TagNode {
    fn default() -> Self {
        Self {
            value: UNDETERMINED,
            low: 0,
        }
    }
}

/// A tag tree over one precinct's code-block grid.
#[derive(Debug)]
pub(crate) struct TagTree {
    nodes: Vec<TagNode>,
    dimensions: [Size<u32>; MAX_LEVELS],
    offsets: [usize; MAX_LEVELS],
    levels: usize,
}

impl TagTree {
    /// Builds an unresolved tag tree over a `size` grid of leaves.
    ///
    /// # Errors
    ///
    /// Returns `LimitExceeded` when the node array passes the caller's
    /// working-memory bound, and `Overflow` if the grid cannot be counted.
    pub(crate) fn new(size: Size<u32>, workspace: &mut Workspace) -> Result<Self, Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "tag tree size",
        };
        let mut dimensions = [Size {
            width: 0,
            height: 0,
        }; MAX_LEVELS];
        let mut offsets = [0usize; MAX_LEVELS];
        let mut extent = Size {
            width: size.width.max(1),
            height: size.height.max(1),
        };
        let mut total = 0usize;
        let mut levels = 0usize;
        loop {
            let slot = dimensions.get_mut(levels).ok_or_else(overflow)?;
            *slot = extent;
            let offset = offsets.get_mut(levels).ok_or_else(overflow)?;
            *offset = total;
            let nodes =
                usize::try_from(u64::from(extent.width).saturating_mul(u64::from(extent.height)))
                    .map_err(|_| overflow())?;
            total = total.checked_add(nodes).ok_or_else(overflow)?;
            levels = levels.checked_add(1).ok_or_else(overflow)?;
            if extent.width == 1 && extent.height == 1 {
                break;
            }
            extent = Size {
                width: extent.width.div_ceil(2),
                height: extent.height.div_ceil(2),
            };
        }
        Ok(Self {
            nodes: workspace.vector(total)?,
            dimensions,
            offsets,
            levels,
        })
    }

    /// Returns whether the leaf's value is below `threshold`.
    ///
    /// Bits are consumed only while the answer is still undecided, so calling
    /// this again with a higher threshold continues where the last call left
    /// off.
    ///
    /// # Errors
    ///
    /// Returns a structural error if the header is truncated or the leaf lies
    /// outside the tree.
    pub(crate) fn decode<S: HeaderSource>(
        &mut self,
        reader: &mut StuffedBitReader<S>,
        leaf: u32,
        threshold: u32,
    ) -> Result<bool, Jpeg2000Error> {
        let mut low = 0u32;
        let mut resolved = UNDETERMINED;
        for level in (0..self.levels).rev() {
            let index = self.node_index(leaf, level)?;
            let node = self.nodes.get_mut(index).ok_or(Jpeg2000Error::Overflow {
                context: "tag tree node index",
            })?;
            if low > node.low {
                node.low = low;
            } else {
                low = node.low;
            }
            while low < threshold && low < node.value {
                if reader.read_bit()? {
                    node.value = low;
                } else {
                    low = low.checked_add(1).ok_or(Jpeg2000Error::Overflow {
                        context: "tag tree value",
                    })?;
                }
            }
            node.low = low;
            resolved = node.value;
        }
        Ok(resolved < threshold)
    }

    /// Returns the node array index of a leaf's ancestor at `level`.
    fn node_index(&self, leaf: u32, level: usize) -> Result<usize, Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "tag tree node index",
        };
        let leaves = self.dimensions.first().ok_or_else(overflow)?;
        let columns = leaves.width.max(1);
        let column = leaf.checked_rem(columns).ok_or_else(overflow)?;
        let row = leaf.checked_div(columns).ok_or_else(overflow)?;
        let shift = u32::try_from(level).map_err(|_| overflow())?;
        let extent = self.dimensions.get(level).ok_or_else(overflow)?;
        let offset = *self.offsets.get(level).ok_or_else(overflow)?;
        let column = column.checked_shr(shift).ok_or_else(overflow)?;
        let row = row.checked_shr(shift).ok_or_else(overflow)?;
        if column >= extent.width || row >= extent.height {
            return Err(overflow());
        }
        usize::try_from(row)
            .ok()
            .and_then(|row| row.checked_mul(usize::try_from(extent.width).ok()?))
            .and_then(|start| start.checked_add(usize::try_from(column).ok()?))
            .and_then(|index| index.checked_add(offset))
            .ok_or_else(overflow)
    }
}
