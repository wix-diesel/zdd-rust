use std::collections::VecDeque;

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

/// Orders edges by a deterministic breadth-first traversal.
///
/// Components start at the smallest unvisited vertex. Incident edges are
/// examined in input order, and an edge is emitted the first time either of
/// its endpoints examines it. Isolated vertices therefore affect neither the
/// order nor the completeness of the result.
#[derive(Clone, Copy, Debug, Default)]
pub struct BfsOrder;

impl EdgeOrdering for BfsOrder {
    fn order(&self, graph: &Graph) -> Result<EdgeOrder, GraphError> {
        let mut incident = vec![Vec::new(); graph.vertex_count()];
        for index in 0..graph.edge_count() {
            let edge = graph.edge_id(index)?;
            let (first, second) = graph.endpoints(edge)?;
            incident[first.index()].push((edge, second));
            incident[second.index()].push((edge, first));
        }

        let mut visited_vertices = vec![false; graph.vertex_count()];
        let mut emitted_edges = vec![false; graph.edge_count()];
        let mut ordered = Vec::with_capacity(graph.edge_count());
        let mut queue = VecDeque::new();

        for root in 0..graph.vertex_count() {
            if visited_vertices[root] {
                continue;
            }
            visited_vertices[root] = true;
            queue.push_back(root);
            while let Some(vertex) = queue.pop_front() {
                for &(edge, neighbor) in &incident[vertex] {
                    if !std::mem::replace(&mut emitted_edges[edge.index()], true) {
                        ordered.push(edge);
                    }
                    if !std::mem::replace(&mut visited_vertices[neighbor.index()], true) {
                        queue.push_back(neighbor.index());
                    }
                }
            }
        }

        EdgeOrder::new(graph, ordered)
    }
}
