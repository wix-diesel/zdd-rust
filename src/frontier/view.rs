use crate::VertexId;

use super::FrontierSlot;

/// A borrowed view of the active frontier at a layer boundary.
///
/// Slots are stable while a vertex is active. The view is valid only during
/// the corresponding [`super::FrontierProblem::canonicalize`] call.
#[derive(Clone, Copy, Debug)]
pub struct FrontierView<'a> {
    slots: &'a [Option<VertexId>],
    width: usize,
}

impl<'a> FrontierView<'a> {
    pub(crate) fn new(slots: &'a [Option<VertexId>], width: usize) -> Self {
        Self { slots, width }
    }

    /// Returns the number of active vertices.
    #[must_use]
    pub fn len(self) -> usize {
        self.width
    }

    /// Returns whether the frontier contains no active vertices.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.width == 0
    }

    /// Returns the active vertex in `slot`, or `None` when the slot is free.
    #[must_use]
    pub fn vertex(self, slot: FrontierSlot) -> Option<VertexId> {
        self.slots.get(slot.index()).copied().flatten()
    }

    /// Iterates over active slot/vertex pairs in increasing slot order.
    pub fn iter(self) -> impl Iterator<Item = (FrontierSlot, VertexId)> + 'a {
        self.slots.iter().enumerate().filter_map(|(index, vertex)| {
            vertex.map(|vertex| (FrontierSlot::from_index(index), vertex))
        })
    }
}
