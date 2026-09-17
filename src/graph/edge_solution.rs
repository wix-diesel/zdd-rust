use std::ops::Deref;
use std::sync::Arc;

use crate::SolutionIterator;

use super::EdgeId;
use super::graph_space::GraphSpaceInner;

/// One owned edge set produced while enumerating an [`EdgeFamily`](super::EdgeFamily).
///
/// Edges occur in the graph space's fixed variable order, not necessarily in
/// ascending [`EdgeId`] order.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeSolution(Vec<EdgeId>);

impl EdgeSolution {
    pub(super) fn new(edges: Vec<EdgeId>) -> Self {
        Self(edges)
    }

    /// Returns the edges in fixed variable order.
    #[must_use]
    pub fn as_slice(&self) -> &[EdgeId] {
        &self.0
    }

    /// Consumes the solution and returns its edge storage.
    #[must_use]
    pub fn into_vec(self) -> Vec<EdgeId> {
        self.0
    }
}

impl AsRef<[EdgeId]> for EdgeSolution {
    fn as_ref(&self) -> &[EdgeId] {
        self.as_slice()
    }
}

impl Deref for EdgeSolution {
    type Target = [EdgeId];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl IntoIterator for EdgeSolution {
    type Item = EdgeId;
    type IntoIter = std::vec::IntoIter<EdgeId>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a EdgeSolution {
    type Item = &'a EdgeId;
    type IntoIter = std::slice::Iter<'a, EdgeId>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// A lazy iterator over typed graph-edge solutions.
///
/// The iterator owns the edge mapping and remains valid after its source
/// graph space, graph, and family are dropped.
pub struct EdgeSolutionIterator {
    pub(super) inner: SolutionIterator,
    pub(super) context: Arc<GraphSpaceInner>,
}

impl Iterator for EdgeSolutionIterator {
    type Item = EdgeSolution;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|solution| {
            EdgeSolution::new(
                solution
                    .into_iter()
                    .map(|variable| self.context.variable_to_edge[variable.index()])
                    .collect(),
            )
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl std::iter::FusedIterator for EdgeSolutionIterator {}
