use std::collections::HashMap;
use std::sync::Arc;

use super::{EdgeId, GraphError, VertexId};

pub(super) struct GraphData {
    pub(super) vertex_count: usize,
    pub(super) edges: Vec<(VertexId, VertexId)>,
}

/// An immutable undirected simple graph with stable, dense input identifiers.
///
/// The vertex count is stored independently from the edges, so isolated
/// vertices and zero-edge graphs are preserved.
#[derive(Clone)]
pub struct Graph {
    pub(super) inner: Arc<GraphData>,
}

impl Graph {
    /// Creates a graph from a vertex count and an input-ordered edge sequence.
    ///
    /// Endpoints are zero-based vertex indices. Out-of-range endpoints, self
    /// loops, and duplicate undirected edges (including reversed duplicates)
    /// are rejected.
    pub fn from_edges<I>(vertex_count: usize, edges: I) -> Result<Self, GraphError>
    where
        I: IntoIterator<Item = (usize, usize)>,
    {
        if vertex_count > u32::MAX as usize {
            return Err(GraphError::CapacityOverflow);
        }
        let iterator = edges.into_iter();
        let mut stored = Vec::with_capacity(iterator.size_hint().0);
        let mut seen = HashMap::new();
        for (edge_index, (first, second)) in iterator.enumerate() {
            Self::validate_endpoint(first, vertex_count)?;
            Self::validate_endpoint(second, vertex_count)?;
            if first == second {
                return Err(GraphError::SelfLoop {
                    edge_index,
                    vertex: first,
                });
            }
            let key = if first < second {
                (first, second)
            } else {
                (second, first)
            };
            if let Some(&first_index) = seen.get(&key) {
                return Err(GraphError::DuplicateEdge {
                    first_index,
                    duplicate_index: edge_index,
                });
            }
            if edge_index > u32::MAX as usize {
                return Err(GraphError::CapacityOverflow);
            }
            seen.insert(key, edge_index);
            stored.push((VertexId(first as u32), VertexId(second as u32)));
        }
        Ok(Self {
            inner: Arc::new(GraphData {
                vertex_count,
                edges: stored,
            }),
        })
    }

    fn validate_endpoint(index: usize, vertex_count: usize) -> Result<(), GraphError> {
        if index < vertex_count {
            Ok(())
        } else {
            Err(GraphError::InvalidVertex {
                index,
                vertex_count,
            })
        }
    }

    /// Returns the number of vertices, including isolated vertices.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.inner.vertex_count
    }

    /// Returns the number of edges.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.inner.edges.len()
    }

    /// Returns a checked vertex identifier.
    pub fn vertex_id(&self, index: usize) -> Result<VertexId, GraphError> {
        Self::validate_endpoint(index, self.vertex_count())?;
        Ok(VertexId(index as u32))
    }

    /// Returns a checked edge identifier in input order.
    pub fn edge_id(&self, index: usize) -> Result<EdgeId, GraphError> {
        if index >= self.edge_count() {
            return Err(GraphError::InvalidEdge {
                index,
                edge_count: self.edge_count(),
            });
        }
        Ok(EdgeId(index as u32))
    }

    /// Returns the endpoints of an edge in their original input orientation.
    pub fn endpoints(&self, edge: EdgeId) -> Result<(VertexId, VertexId), GraphError> {
        self.inner
            .edges
            .get(edge.index())
            .copied()
            .ok_or(GraphError::InvalidEdge {
                index: edge.index(),
                edge_count: self.edge_count(),
            })
    }
}
