use super::{EdgeId, Graph, GraphError};

/// A validated permutation of all edges in a graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeOrder(Vec<EdgeId>);

impl EdgeOrder {
    /// Validates and stores a complete edge permutation.
    pub fn new<I>(graph: &Graph, edges: I) -> Result<Self, GraphError>
    where
        I: IntoIterator<Item = EdgeId>,
    {
        let edges: Vec<_> = edges.into_iter().collect();
        if edges.len() != graph.edge_count() {
            return Err(GraphError::InvalidEdgeOrderLength {
                expected: graph.edge_count(),
                actual: edges.len(),
            });
        }
        let mut seen = vec![false; graph.edge_count()];
        for &edge in &edges {
            if edge.index() >= graph.edge_count() {
                return Err(GraphError::InvalidEdge {
                    index: edge.index(),
                    edge_count: graph.edge_count(),
                });
            }
            if std::mem::replace(&mut seen[edge.index()], true) {
                return Err(GraphError::DuplicateOrderedEdge { edge });
            }
        }
        Ok(Self(edges))
    }

    /// Returns the edge identifiers in variable order.
    #[must_use]
    pub fn as_slice(&self) -> &[EdgeId] {
        &self.0
    }
}

/// Strategy for selecting the fixed edge order of a graph space.
pub trait EdgeOrdering {
    /// Returns a validated order for `graph`.
    fn order(&self, graph: &Graph) -> Result<EdgeOrder, GraphError>;
}

impl EdgeOrdering for EdgeOrder {
    fn order(&self, graph: &Graph) -> Result<EdgeOrder, GraphError> {
        Self::new(graph, self.0.iter().copied())
    }
}

/// Uses the graph's edge input order.
#[derive(Clone, Copy, Debug, Default)]
pub struct InputOrder;

impl EdgeOrdering for InputOrder {
    fn order(&self, graph: &Graph) -> Result<EdgeOrder, GraphError> {
        EdgeOrder::new(
            graph,
            (0..graph.edge_count()).map(|index| EdgeId(index as u32)),
        )
    }
}
