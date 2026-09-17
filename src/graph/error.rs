use std::error::Error as StdError;
use std::fmt;

use crate::Error;

use super::EdgeId;

/// Error returned by graph construction or a typed edge-family operation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GraphError {
    /// An endpoint is outside the graph's fixed vertex range.
    #[non_exhaustive]
    InvalidVertex {
        /// The rejected endpoint index.
        index: usize,
        /// The graph's number of vertices.
        vertex_count: usize,
    },
    /// An edge identifier is outside the graph's fixed edge range.
    #[non_exhaustive]
    InvalidEdge {
        /// The rejected edge index.
        index: usize,
        /// The graph's number of edges.
        edge_count: usize,
    },
    /// A variable cannot be resolved through this graph space's edge mapping.
    #[non_exhaustive]
    InvalidVariableMap {
        /// The rejected variable index.
        variable_index: usize,
        /// The number of variables in the graph space.
        variable_count: usize,
    },
    /// A self loop was supplied to an undirected simple graph.
    #[non_exhaustive]
    SelfLoop {
        /// The input position of the rejected edge.
        edge_index: usize,
        /// The repeated endpoint.
        vertex: usize,
    },
    /// An undirected edge duplicates an earlier edge.
    #[non_exhaustive]
    DuplicateEdge {
        /// The input position of the first occurrence.
        first_index: usize,
        /// The input position of the duplicate occurrence.
        duplicate_index: usize,
    },
    /// An edge order does not contain exactly one entry per graph edge.
    #[non_exhaustive]
    InvalidEdgeOrderLength {
        /// The graph's number of edges.
        expected: usize,
        /// The number of supplied edge identifiers.
        actual: usize,
    },
    /// An edge occurs more than once in an edge order.
    #[non_exhaustive]
    DuplicateOrderedEdge {
        /// The duplicated edge.
        edge: EdgeId,
    },
    /// Two edge families belong to different graph spaces.
    #[non_exhaustive]
    ContextMismatch {},
    /// An identifier or collection cannot be represented by this implementation.
    CapacityOverflow,
    /// The underlying graph-independent family operation failed.
    Family(Error),
}

impl fmt::Display for GraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVertex {
                index,
                vertex_count,
            } => write!(
                formatter,
                "vertex index {index} is outside a graph of {vertex_count} vertices"
            ),
            Self::InvalidEdge { index, edge_count } => write!(
                formatter,
                "edge index {index} is outside a graph of {edge_count} edges"
            ),
            Self::InvalidVariableMap {
                variable_index,
                variable_count,
            } => write!(
                formatter,
                "variable index {variable_index} cannot be mapped in a graph space of {variable_count} variables"
            ),
            Self::SelfLoop { edge_index, vertex } => {
                write!(
                    formatter,
                    "edge {edge_index} is a self loop at vertex {vertex}"
                )
            }
            Self::DuplicateEdge {
                first_index,
                duplicate_index,
            } => write!(
                formatter,
                "edge {duplicate_index} duplicates undirected edge {first_index}"
            ),
            Self::InvalidEdgeOrderLength { expected, actual } => write!(
                formatter,
                "edge order has {actual} entries but the graph has {expected} edges"
            ),
            Self::DuplicateOrderedEdge { edge } => write!(
                formatter,
                "edge {} occurs more than once in the order",
                edge.index()
            ),
            Self::ContextMismatch {} => {
                formatter.write_str("edge families belong to different graph spaces")
            }
            Self::CapacityOverflow => {
                formatter.write_str("graph size exceeds the supported identifier capacity")
            }
            Self::Family(error) => error.fmt(formatter),
        }
    }
}

impl StdError for GraphError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Family(error) => Some(error),
            _ => None,
        }
    }
}

impl From<Error> for GraphError {
    fn from(error: Error) -> Self {
        Self::Family(error)
    }
}
