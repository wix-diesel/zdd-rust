//! Immutable graph data and typed edge-family APIs.

mod compaction;
mod edge_family;
mod edge_solution;
mod error;
mod graph_data;
mod graph_space;
mod ids;
mod ordering;

pub use edge_family::{EdgeCardinalityFilter, EdgeFamily};
pub use edge_solution::{EdgeSolution, EdgeSolutionIterator};
pub use error::GraphError;
pub use graph_data::Graph;
pub use graph_space::{GraphSpace, GraphSpaceBuilder};
pub use ids::{EdgeId, VertexId};
pub use ordering::{BfsOrder, EdgeOrder, EdgeOrdering, InputOrder};
