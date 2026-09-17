use std::ops::ControlFlow;
use std::sync::Arc;

use crate::{BigUint, CountError, CountIndex, OperationReport, QueryError, QueryLimits, SetFamily};

use super::graph_space::GraphSpaceInner;
use super::{EdgeId, EdgeSolutionIterator, Graph, GraphError};

/// An immutable family whose elements are original graph edge identifiers.
#[derive(Clone)]
pub struct EdgeFamily {
    pub(super) context: Arc<GraphSpaceInner>,
    pub(super) family: SetFamily,
}

impl EdgeFamily {
    /// Returns the underlying graph-independent family as a read-only view.
    #[must_use]
    pub fn as_set_family(&self) -> &SetFamily {
        &self.family
    }

    /// Returns the graph whose edge identifiers this family uses.
    #[must_use]
    pub fn graph(&self) -> &Graph {
        &self.context.graph
    }

    /// Returns the exact number of edge sets in this family.
    #[must_use]
    pub fn count(&self) -> BigUint {
        self.family.count()
    }

    /// Returns the number of edge sets when it fits in `u128`.
    pub fn try_count_u128(&self) -> Result<u128, CountError> {
        self.family.try_count_u128()
    }

    /// Builds a reusable exact-count index under explicit query limits.
    pub fn count_index(&self, limits: &QueryLimits) -> Result<CountIndex, QueryError> {
        self.family.count_index(limits)
    }

    /// Lazily enumerates edge sets in fixed variable order.
    #[must_use]
    pub fn iter(&self) -> EdgeSolutionIterator {
        EdgeSolutionIterator {
            inner: self.family.iter(),
            context: Arc::clone(&self.context),
        }
    }

    /// Visits edge sets with one reused edge buffer.
    pub fn visit_solutions<B>(
        &self,
        mut visitor: impl FnMut(&[EdgeId]) -> ControlFlow<B>,
    ) -> ControlFlow<B> {
        let mapping = &self.context.variable_to_edge;
        let mut edges = Vec::new();
        self.family.visit_solutions(|solution| {
            edges.clear();
            edges.extend(solution.iter().map(|variable| mapping[variable.index()]));
            visitor(&edges)
        })
    }

    /// Returns whether this family contains no edge sets.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.family.is_empty()
    }

    /// Returns whether two families in the same graph space are equivalent.
    pub fn equivalent(&self, other: &Self) -> Result<bool, GraphError> {
        self.ensure_same_context(other)?;
        Ok(self.family.equivalent(&other.family)?)
    }

    /// Returns whether every edge set in this family occurs in `other`.
    pub fn is_subset_of(&self, other: &Self) -> Result<bool, GraphError> {
        self.ensure_same_context(other)?;
        Ok(self.family.is_subset_of(&other.family)?)
    }

    /// Returns whether `edges` is a member of this family.
    pub fn contains(&self, edges: &[EdgeId]) -> Result<bool, GraphError> {
        let variables = self.map_edges(edges.iter().copied())?;
        Ok(self.family.contains(&variables)?)
    }

    /// Keeps only edge sets containing `edge`.
    pub fn filter_contains(&self, edge: EdgeId) -> Result<Self, GraphError> {
        let variable = self.variable(edge)?;
        Ok(self.wrap(self.family.filter_contains(variable)?))
    }

    /// Keeps only edge sets that do not contain `edge`.
    pub fn filter_excludes(&self, edge: EdgeId) -> Result<Self, GraphError> {
        let variable = self.variable(edge)?;
        Ok(self.wrap(self.family.filter_excludes(variable)?))
    }

    /// Keeps only edge sets that are subsets of `edges`.
    pub fn filter_subsets_of(&self, edges: &[EdgeId]) -> Result<Self, GraphError> {
        let variables = self.map_edges(edges.iter().copied())?;
        Ok(self.wrap(self.family.filter_subsets_of(&variables)?))
    }

    /// Keeps only edge sets that are supersets of `edges`.
    pub fn filter_supersets_of(&self, edges: &[EdgeId]) -> Result<Self, GraphError> {
        let variables = self.map_edges(edges.iter().copied())?;
        Ok(self.wrap(self.family.filter_supersets_of(&variables)?))
    }

    /// Starts a filter over the number of edges in each solution.
    #[must_use]
    pub fn cardinality(&self) -> EdgeCardinalityFilter<'_> {
        EdgeCardinalityFilter { family: self }
    }

    /// Returns the union of two families in the same graph space.
    pub fn union(&self, other: &Self) -> Result<Self, GraphError> {
        Ok(self.union_with_stats(other)?.value)
    }

    /// Returns the union and diagnostic statistics.
    pub fn union_with_stats(&self, other: &Self) -> Result<OperationReport<Self>, GraphError> {
        self.binary_with_stats(other, |left, right| left.union_with_stats(right))
    }

    /// Returns the intersection of two families in the same graph space.
    pub fn intersection(&self, other: &Self) -> Result<Self, GraphError> {
        Ok(self.intersection_with_stats(other)?.value)
    }

    /// Returns the intersection and diagnostic statistics.
    pub fn intersection_with_stats(
        &self,
        other: &Self,
    ) -> Result<OperationReport<Self>, GraphError> {
        self.binary_with_stats(other, |left, right| left.intersection_with_stats(right))
    }

    /// Returns edge sets in this family that are absent from `other`.
    pub fn difference(&self, other: &Self) -> Result<Self, GraphError> {
        Ok(self.difference_with_stats(other)?.value)
    }

    /// Returns the difference and diagnostic statistics.
    pub fn difference_with_stats(&self, other: &Self) -> Result<OperationReport<Self>, GraphError> {
        self.binary_with_stats(other, |left, right| left.difference_with_stats(right))
    }

    /// Returns edge sets occurring in exactly one of the two families.
    pub fn symmetric_difference(&self, other: &Self) -> Result<Self, GraphError> {
        Ok(self.symmetric_difference_with_stats(other)?.value)
    }

    /// Returns the symmetric difference and diagnostic statistics.
    pub fn symmetric_difference_with_stats(
        &self,
        other: &Self,
    ) -> Result<OperationReport<Self>, GraphError> {
        self.binary_with_stats(other, |left, right| {
            left.symmetric_difference_with_stats(right)
        })
    }

    fn binary_with_stats(
        &self,
        other: &Self,
        operation: impl FnOnce(
            &SetFamily,
            &SetFamily,
        ) -> Result<OperationReport<SetFamily>, crate::Error>,
    ) -> Result<OperationReport<Self>, GraphError> {
        self.ensure_same_context(other)?;
        let report = operation(&self.family, &other.family)?;
        Ok(OperationReport {
            value: self.wrap(report.value),
            stats: report.stats,
        })
    }

    fn ensure_same_context(&self, other: &Self) -> Result<(), GraphError> {
        if Arc::ptr_eq(&self.context, &other.context) {
            Ok(())
        } else {
            Err(GraphError::ContextMismatch {})
        }
    }

    fn variable(&self, edge: EdgeId) -> Result<crate::VariableId, GraphError> {
        self.context
            .edge_to_variable
            .get(edge.index())
            .copied()
            .ok_or(GraphError::InvalidEdge {
                index: edge.index(),
                edge_count: self.context.graph.edge_count(),
            })
    }

    fn map_edges(
        &self,
        edges: impl IntoIterator<Item = EdgeId>,
    ) -> Result<Vec<crate::VariableId>, GraphError> {
        edges.into_iter().map(|edge| self.variable(edge)).collect()
    }

    fn wrap(&self, family: SetFamily) -> Self {
        Self {
            context: Arc::clone(&self.context),
            family,
        }
    }
}

/// Builder-like view for filtering an edge family by solution cardinality.
pub struct EdgeCardinalityFilter<'a> {
    family: &'a EdgeFamily,
}

impl EdgeCardinalityFilter<'_> {
    /// Keeps edge sets containing exactly `count` edges.
    pub fn exactly(&self, count: usize) -> Result<EdgeFamily, GraphError> {
        Ok(self
            .family
            .wrap(self.family.family.cardinality().exactly(count)?))
    }

    /// Keeps edge sets containing at most `count` edges.
    pub fn at_most(&self, count: usize) -> Result<EdgeFamily, GraphError> {
        Ok(self
            .family
            .wrap(self.family.family.cardinality().at_most(count)?))
    }

    /// Keeps edge sets containing at least `count` edges.
    pub fn at_least(&self, count: usize) -> Result<EdgeFamily, GraphError> {
        Ok(self
            .family
            .wrap(self.family.family.cardinality().at_least(count)?))
    }

    /// Keeps edge sets whose cardinality lies in the inclusive `range`.
    pub fn between(
        &self,
        range: std::ops::RangeInclusive<usize>,
    ) -> Result<EdgeFamily, GraphError> {
        Ok(self
            .family
            .wrap(self.family.family.cardinality().between(range)?))
    }
}
