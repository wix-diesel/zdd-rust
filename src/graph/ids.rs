/// A vertex identifier within a [`Graph`](super::Graph).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VertexId(pub(super) u32);

impl VertexId {
    /// Returns this vertex's position in the graph's input vertex range.
    #[must_use]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// An edge identifier within a [`Graph`](super::Graph).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeId(pub(super) u32);

impl EdgeId {
    /// Returns this edge's position in the graph's input edge sequence.
    #[must_use]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}
