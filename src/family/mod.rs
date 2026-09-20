//! Public, graph-independent set-family API.

use std::error::Error as StdError;
use std::fmt;
use std::iter::FusedIterator;
use std::ops::{ControlFlow, Deref};
use std::sync::Arc;

use num_bigint::BigUint;

use crate::zdd::{
    ApplyError, ApplyOp, ApplyStats, CreateError, FilterSpec, QUERY_ONE, QUERY_ZERO, QueryDag,
    Root, ZddManager,
};

mod api_types;
mod cardinality_filter;
mod count_index;
#[cfg(feature = "sampling")]
mod sampling;
mod set_family;
mod set_operations;
mod solution;
mod solution_iterator;
mod space;

pub use api_types::{
    CancellationToken, CountError, Error, LimitKind, Limits, OperationReport, OperationStats,
    QueryError, QueryLimits, QueryStats, SpaceStats, VariableId,
};

struct SpaceInner {
    variable_count: usize,
    limits: Limits,
    manager: ZddManager,
}

/// A fixed variable universe and its shared ZDD manager.
#[derive(Clone)]
pub struct FamilySpace {
    inner: Arc<SpaceInner>,
}

/// Builder for a [`FamilySpace`].
pub struct FamilySpaceBuilder {
    variable_count: usize,
    limits: Limits,
}

/// An immutable family of sets in a [`FamilySpace`].
#[derive(Clone)]
pub struct SetFamily {
    space: Arc<SpaceInner>,
    root: Root,
}

/// One owned set produced while enumerating a [`SetFamily`].
///
/// Elements are ordered by their fixed variable order and occur at most once.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Solution(Vec<VariableId>);

/// A lazy, deterministic iterator over the sets in a [`SetFamily`].
///
/// The iterator owns a query-local DAG snapshot and remains valid after its
/// source family and space are dropped. Each yielded [`Solution`] owns a new
/// element buffer; use [`SetFamily::visit_solutions`] to reuse one buffer.
pub struct SolutionIterator {
    traversal: SolutionTraversal,
}

struct SolutionTraversal {
    dag: QueryDag,
    stack: Vec<TraversalStep>,
    current: Vec<VariableId>,
}

enum TraversalStep {
    Visit(usize),
    Include { reference: usize, variable: u32 },
    Remove,
}

/// An owned query-local DAG and exact count for every reachable branch.
///
/// The index is detached from its source [`SetFamily`] and [`FamilySpace`], so
/// it remains valid after both are dropped. It can be reused by later sampling
/// and rank operations without retaining an unbounded manager-global cache.
#[derive(Clone, Debug)]
pub struct CountIndex {
    dag: QueryDag,
    counts: Vec<Option<BigUint>>,
    stats: QueryStats,
}

enum CountWork {
    Visit(usize),
    Finish(usize),
}

/// Builder-like view for filtering a family by the number of selected elements.
pub struct CardinalityFilter<'a> {
    family: &'a SetFamily,
}

#[cfg(test)]
#[path = "../../tests/internal/family.rs"]
mod tests;
