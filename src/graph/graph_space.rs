use std::convert::Infallible;
use std::sync::Arc;

use crate::{BuildError, FamilySpace, FrontierBuilder, Limits, SetFamily, SpaceStats, VariableId};

use super::{EdgeFamily, EdgeId, EdgeOrder, EdgeOrdering, Graph, GraphError, InputOrder, VertexId};

pub(super) struct GraphSpaceInner {
    pub(super) graph: Graph,
    pub(super) family_space: FamilySpace,
    pub(super) edge_to_variable: Vec<VariableId>,
    pub(super) variable_to_edge: Vec<EdgeId>,
}

/// A graph, fixed edge order, and graph-independent family space.
#[derive(Clone)]
pub struct GraphSpace {
    pub(super) inner: Arc<GraphSpaceInner>,
}

/// Builder for a [`GraphSpace`].
pub struct GraphSpaceBuilder<'a> {
    graph: &'a Graph,
    ordering: Box<dyn EdgeOrdering>,
    limits: Limits,
}

impl GraphSpace {
    /// Creates a graph space using the graph's input edge order.
    pub fn new(graph: &Graph) -> Result<Self, GraphError> {
        Self::builder(graph).build()
    }

    /// Starts configuring a graph space.
    #[must_use]
    pub fn builder(graph: &Graph) -> GraphSpaceBuilder<'_> {
        GraphSpaceBuilder {
            graph,
            ordering: Box::new(InputOrder),
            limits: Limits::default(),
        }
    }

    /// Returns the immutable graph owned by this space.
    #[must_use]
    pub fn graph(&self) -> &Graph {
        &self.inner.graph
    }

    /// Returns the underlying graph-independent family space.
    #[must_use]
    pub fn as_family_space(&self) -> &FamilySpace {
        &self.inner.family_space
    }

    /// Returns the variable assigned to an original edge identifier.
    pub fn variable_for_edge(&self, edge: EdgeId) -> Result<VariableId, GraphError> {
        self.inner
            .edge_to_variable
            .get(edge.index())
            .copied()
            .ok_or(GraphError::InvalidEdge {
                index: edge.index(),
                edge_count: self.graph().edge_count(),
            })
    }

    /// Returns the original edge assigned to a family-space variable.
    pub fn edge_for_variable(&self, variable: VariableId) -> Result<EdgeId, GraphError> {
        self.inner
            .variable_to_edge
            .get(variable.index())
            .copied()
            .ok_or(GraphError::InvalidVariableMap {
                variable_index: variable.index(),
                variable_count: self.graph().edge_count(),
            })
    }

    /// Returns a snapshot of the underlying manager's statistics.
    #[must_use]
    pub fn stats(&self) -> SpaceStats {
        self.inner.family_space.stats()
    }

    /// Returns the empty edge family (ZERO).
    #[must_use]
    pub fn empty(&self) -> EdgeFamily {
        self.family(self.inner.family_space.empty())
    }

    /// Returns the edge family containing only the empty edge set (ONE).
    #[must_use]
    pub fn unit(&self) -> EdgeFamily {
        self.family(self.inner.family_space.unit())
    }

    /// Returns the family containing every subset of graph edges.
    pub fn powerset(&self) -> Result<EdgeFamily, GraphError> {
        Ok(self.family(self.inner.family_space.powerset()?))
    }

    /// Returns the family of all matchings in this graph.
    ///
    /// A matching contains no two edges with a shared endpoint. The empty
    /// matching is always included, including for graphs with no edges or
    /// only isolated vertices. This method returns all matchings, not only
    /// maximal or maximum-cardinality matchings.
    pub fn matchings(&self) -> Result<EdgeFamily, BuildError<Infallible>> {
        FrontierBuilder::new(self).build(crate::problems::MatchingProblem)
    }

    /// Returns the family of all vertex-simple paths from `source` to `target`.
    ///
    /// Each path is represented by its edge set, so its orientation and
    /// traversal order do not create duplicate solutions. Equal or out-of-range
    /// endpoints are reported as problem errors. A valid pair with no path
    /// produces the empty family.
    pub fn paths(
        &self,
        source: VertexId,
        target: VertexId,
    ) -> Result<EdgeFamily, BuildError<GraphError>> {
        FrontierBuilder::new(self).build(crate::problems::PathProblem::new(source, target))
    }

    /// Builds an edge family from explicit edge sets.
    ///
    /// Edge order, duplicate edges, and duplicate sets are normalized.
    pub fn from_edge_sets<I, S>(&self, sets: I) -> Result<EdgeFamily, GraphError>
    where
        I: IntoIterator<Item = S>,
        S: IntoIterator<Item = EdgeId>,
    {
        let variable_sets = sets
            .into_iter()
            .map(|set| {
                set.into_iter()
                    .map(|edge| self.variable_for_edge(edge))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self.family(self.inner.family_space.from_sets(variable_sets)?))
    }

    pub(crate) fn family(&self, family: SetFamily) -> EdgeFamily {
        EdgeFamily {
            context: Arc::clone(&self.inner),
            family,
        }
    }
}

impl GraphSpaceBuilder<'_> {
    /// Replaces the input edge order with `ordering`.
    #[must_use]
    pub fn ordering(mut self, ordering: impl EdgeOrdering + 'static) -> Self {
        self.ordering = Box::new(ordering);
        self
    }

    /// Replaces the default family-space resource limits.
    #[must_use]
    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Creates the configured graph space.
    pub fn build(self) -> Result<GraphSpace, GraphError> {
        let order: EdgeOrder = self.ordering.order(self.graph)?;
        let family_space = FamilySpace::builder(self.graph.edge_count())
            .limits(self.limits)
            .build()?;
        let mut edge_to_variable: Vec<Option<VariableId>> = vec![None; self.graph.edge_count()];
        let mut variable_to_edge = Vec::with_capacity(self.graph.edge_count());
        for (level, &edge) in order.as_slice().iter().enumerate() {
            let variable = family_space.variable(level)?;
            edge_to_variable[edge.index()] = Some(variable);
            variable_to_edge.push(edge);
        }
        let edge_to_variable = edge_to_variable
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(GraphError::CapacityOverflow)?;

        Ok(GraphSpace {
            inner: Arc::new(GraphSpaceInner {
                graph: self.graph.clone(),
                family_space,
                edge_to_variable,
                variable_to_edge,
            }),
        })
    }
}
