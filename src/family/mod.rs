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

/// An element identifier in a [`FamilySpace`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariableId(u32);

impl VariableId {
    /// Returns the element's position in its space's input universe.
    #[must_use]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Space-wide resource limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum number of live nonterminal nodes in the shared manager.
    pub max_live_nodes: usize,
    /// Maximum distinct memo entries retained by one symbolic operation.
    pub max_operation_memo_entries: usize,
    /// Maximum distinct canonical states retained in one frontier layer.
    pub max_frontier_states: usize,
    /// Maximum include/exclude transitions attempted by one frontier build.
    pub max_frontier_transitions: usize,
    /// Maximum entries in the adapter-owned shared computed cache.
    ///
    /// A value of zero disables the cache.
    pub shared_cache_entries: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_live_nodes: 1_000_000,
            max_operation_memo_entries: 1_000_000,
            max_frontier_states: 1_000_000,
            max_frontier_transitions: 10_000_000,
            shared_cache_entries: 262_144,
        }
    }
}

/// Per-query limits for an owned local DAG and its exact partial counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryLimits {
    /// Maximum number of distinct reachable nonterminal nodes in the snapshot.
    pub max_snapshot_nodes: usize,
    /// Maximum sum of `BigUint::bits()` over all retained partial counts.
    ///
    /// This is a deterministic logical-size limit, not an allocator or RSS limit.
    pub max_total_count_bits: usize,
}

impl Default for QueryLimits {
    fn default() -> Self {
        Self {
            max_snapshot_nodes: 1_000_000,
            max_total_count_bits: 67_108_864,
        }
    }
}

/// The resource whose limit was exceeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LimitKind {
    /// Live nonterminal nodes in the shared manager.
    Node,
    /// Distinct keys retained by one operation-local memo table.
    OperationMemo,
    /// Distinct nonterminal nodes copied into a query-local DAG.
    QueryNodes,
    /// Logical bits in all exact partial counts retained by a query.
    CountBits,
}

/// Statistics collected while constructing a [`CountIndex`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct QueryStats {
    /// Distinct reachable nonterminal nodes copied into the local snapshot.
    pub snapshot_nodes: usize,
    /// Sum of `BigUint::bits()` for all retained partial counts.
    pub total_count_bits: usize,
    /// Largest `BigUint::bits()` value among retained partial counts.
    pub max_count_bits: usize,
}

/// Error returned while building a bounded query index.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum QueryError {
    /// A configured query resource limit was exceeded.
    #[non_exhaustive]
    LimitExceeded {
        /// The query resource that could not be added.
        kind: LimitKind,
        /// The configured maximum value.
        limit: usize,
        /// The value that the query attempted to reach.
        attempted: usize,
        /// Work completed before the limit was encountered.
        stats: QueryStats,
    },
}

impl fmt::Display for QueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded {
                kind,
                limit,
                attempted,
                ..
            } => write!(
                formatter,
                "{kind:?} limit {limit} exceeded while attempting to use {attempted}"
            ),
        }
    }
}

impl StdError for QueryError {}

/// Error returned by a fixed-width count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CountError {
    /// The exact family size does not fit in `u128`.
    Overflow,
}

impl fmt::Display for CountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow => formatter.write_str("the exact family count exceeds u128"),
        }
    }
}

impl StdError for CountError {}

/// Statistics collected for one set-family operation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct OperationStats {
    /// Live manager nodes immediately before the operation.
    pub nodes_before: usize,
    /// Live manager nodes immediately after success or failure.
    pub nodes_after: usize,
    /// New canonical nodes created by this operation, including nodes later collected.
    pub nodes_created: usize,
    /// Peak number of entries in the operation-local memo table.
    pub peak_operation_memo_entries: usize,
    /// Number of operation-local memo hits.
    pub operation_memo_hits: usize,
    /// Number of shared computed-cache hits.
    pub shared_cache_hits: usize,
    /// Number of shared computed-cache misses.
    pub shared_cache_misses: usize,
    /// Number of cooperative cancellation checks.
    pub cancellation_checks: usize,
}

/// A successful operation value together with its diagnostic statistics.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OperationReport<T> {
    /// The operation result.
    pub value: T,
    /// Work performed by the operation.
    pub stats: OperationStats,
}

/// A snapshot of manager-wide resource and cache statistics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SpaceStats {
    /// Current live nonterminal node count.
    pub live_nodes: usize,
    /// Highest observed live nonterminal node count.
    pub peak_live_nodes: usize,
    /// Cumulative canonical nodes created, including nodes later collected.
    pub nodes_created: usize,
    /// Current shared computed-cache entry count.
    pub shared_cache_entries: usize,
    /// Cumulative shared computed-cache hits.
    pub shared_cache_hits: usize,
    /// Cumulative shared computed-cache misses.
    pub shared_cache_misses: usize,
    /// Cumulative shared computed-cache evictions.
    pub shared_cache_evictions: usize,
    /// Number of backend garbage collections.
    pub garbage_collections: u64,
}

impl OperationStats {
    fn node_construction(nodes_before: usize, nodes_after: usize, nodes_created: usize) -> Self {
        Self {
            nodes_before,
            nodes_after,
            nodes_created,
            ..Self::default()
        }
    }
}

/// Error returned by set-family construction.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// An element index is outside the fixed universe.
    #[non_exhaustive]
    InvalidElement {
        /// The rejected element index.
        index: usize,
        /// The number of variables in the destination universe.
        variable_count: usize,
    },
    /// A closed cardinality range has its lower endpoint above its upper endpoint.
    #[non_exhaustive]
    InvalidRange {
        /// Inclusive lower endpoint.
        start: usize,
        /// Inclusive upper endpoint.
        end: usize,
    },
    /// Two families belong to different spaces.
    #[non_exhaustive]
    ContextMismatch {},
    /// A configured resource limit was exceeded.
    #[non_exhaustive]
    LimitExceeded {
        /// The resource that could not be added.
        kind: LimitKind,
        /// The configured maximum value.
        limit: usize,
        /// The value that the operation attempted to reach.
        attempted: usize,
        /// Work completed before the limit was encountered.
        stats: OperationStats,
    },
    /// A public capacity cannot be represented by the selected backend.
    CapacityOverflow,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidElement {
                index,
                variable_count,
            } => write!(
                formatter,
                "element index {index} is outside a universe of {variable_count} variables"
            ),
            Self::InvalidRange { start, end } => {
                write!(formatter, "invalid closed range {start}..={end}")
            }
            Self::LimitExceeded {
                kind,
                limit,
                attempted,
                ..
            } => write!(
                formatter,
                "{kind:?} limit {limit} exceeded while attempting to use {attempted} entries"
            ),
            Self::CapacityOverflow => {
                write!(formatter, "capacity cannot be represented by the backend")
            }
            Self::ContextMismatch {} => write!(formatter, "families belong to different spaces"),
        }
    }
}

impl StdError for Error {}

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

impl Solution {
    /// Returns the elements in fixed variable order.
    #[must_use]
    pub fn as_slice(&self) -> &[VariableId] {
        &self.0
    }

    /// Consumes the solution and returns its element storage.
    #[must_use]
    pub fn into_vec(self) -> Vec<VariableId> {
        self.0
    }
}

impl AsRef<[VariableId]> for Solution {
    fn as_ref(&self) -> &[VariableId] {
        self.as_slice()
    }
}

impl Deref for Solution {
    type Target = [VariableId];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl IntoIterator for Solution {
    type Item = VariableId;
    type IntoIter = std::vec::IntoIter<VariableId>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Solution {
    type Item = &'a VariableId;
    type IntoIter = std::slice::Iter<'a, VariableId>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

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

impl FamilySpace {
    /// Creates a space with `variable_count` variables in input order.
    pub fn new(variable_count: usize) -> Result<Self, Error> {
        Self::builder(variable_count).build()
    }

    /// Starts configuring a space with `variable_count` variables.
    #[must_use]
    pub fn builder(variable_count: usize) -> FamilySpaceBuilder {
        FamilySpaceBuilder {
            variable_count,
            limits: Limits::default(),
        }
    }

    /// Returns a checked identifier for an element in this universe.
    pub fn variable(&self, index: usize) -> Result<VariableId, Error> {
        if index >= self.inner.variable_count {
            return Err(Error::InvalidElement {
                index,
                variable_count: self.inner.variable_count,
            });
        }
        let index = u32::try_from(index).map_err(|_| Error::CapacityOverflow)?;
        Ok(VariableId(index))
    }

    /// Returns the empty family (ZERO).
    #[must_use]
    pub fn empty(&self) -> SetFamily {
        self.family(self.inner.manager.empty())
    }

    /// Returns the family containing only the empty set (ONE).
    #[must_use]
    pub fn unit(&self) -> SetFamily {
        self.family(self.inner.manager.unit())
    }

    /// Returns the family containing every subset of this universe.
    pub fn powerset(&self) -> Result<SetFamily, Error> {
        Ok(self.family(self.inner.manager.powerset()))
    }

    /// Returns a snapshot of manager-wide statistics.
    #[must_use]
    pub fn stats(&self) -> SpaceStats {
        let manager = self.inner.manager.stats();
        SpaceStats {
            live_nodes: self.inner.manager.inner_node_count(),
            peak_live_nodes: manager.peak_live_nodes,
            nodes_created: manager.nodes_created,
            shared_cache_entries: manager.shared_cache_entries,
            shared_cache_hits: manager.shared_cache_hits,
            shared_cache_misses: manager.shared_cache_misses,
            shared_cache_evictions: manager.shared_cache_evictions,
            garbage_collections: manager.gc_count,
        }
    }

    /// Builds a family from explicit sets.
    ///
    /// Element order, duplicate elements, and duplicate sets are normalized.
    pub fn from_sets<I, S>(&self, sets: I) -> Result<SetFamily, Error>
    where
        I: IntoIterator<Item = S>,
        S: IntoIterator<Item = VariableId>,
    {
        let mut normalized = Vec::new();
        for set in sets {
            let mut set: Vec<u32> = set
                .into_iter()
                .map(|element| {
                    if element.index() >= self.inner.variable_count {
                        Err(Error::InvalidElement {
                            index: element.index(),
                            variable_count: self.inner.variable_count,
                        })
                    } else {
                        Ok(element.0)
                    }
                })
                .collect::<Result<_, _>>()?;
            set.sort_unstable();
            set.dedup();
            normalized.push(set);
        }
        normalized.sort_unstable();
        normalized.dedup();

        let nodes_before = self.inner.manager.inner_node_count();
        let root = match self.inner.manager.build_from_sets(&normalized) {
            Ok((root, _)) => root,
            Err(error) => {
                let nodes_after = self.inner.manager.inner_node_count();
                return Err(Error::LimitExceeded {
                    kind: LimitKind::Node,
                    limit: self.inner.limits.max_live_nodes,
                    attempted: self.inner.limits.max_live_nodes.saturating_add(1),
                    stats: OperationStats::node_construction(
                        nodes_before,
                        nodes_after,
                        error.nodes_created,
                    ),
                });
            }
        };
        Ok(self.family(root))
    }

    fn family(&self, root: Root) -> SetFamily {
        SetFamily {
            space: Arc::clone(&self.inner),
            root,
        }
    }
}

impl FamilySpaceBuilder {
    /// Replaces the default space-wide limits.
    #[must_use]
    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Creates the configured space.
    pub fn build(self) -> Result<FamilySpace, Error> {
        // OxiDD initializes one tautology and one singleton node per variable.
        // Validate this before manager creation because tautology initialization
        // cannot report allocation failure without aborting.
        let initial_nodes = self
            .variable_count
            .checked_mul(2)
            .ok_or(Error::CapacityOverflow)?;
        if initial_nodes > self.limits.max_live_nodes {
            return Err(Error::LimitExceeded {
                kind: LimitKind::Node,
                limit: self.limits.max_live_nodes,
                attempted: initial_nodes,
                stats: OperationStats::default(),
            });
        }

        let manager = ZddManager::new(
            self.variable_count,
            self.limits.max_live_nodes,
            self.limits.shared_cache_entries,
        )
        .map_err(|error| match error {
            CreateError::TooManyVariables | CreateError::NodeCapacityTooLarge => {
                Error::CapacityOverflow
            }
        })?;

        Ok(FamilySpace {
            inner: Arc::new(SpaceInner {
                variable_count: self.variable_count,
                limits: self.limits,
                manager,
            }),
        })
    }
}

impl SetFamily {
    /// Returns the union of two families in the same space.
    pub fn union(&self, other: &Self) -> Result<Self, Error> {
        Ok(self.union_with_stats(other)?.value)
    }

    /// Returns the union and diagnostic statistics.
    pub fn union_with_stats(&self, other: &Self) -> Result<OperationReport<Self>, Error> {
        self.binary_with_stats(other, ApplyOp::Union)
    }

    /// Returns the intersection of two families in the same space.
    pub fn intersection(&self, other: &Self) -> Result<Self, Error> {
        Ok(self.intersection_with_stats(other)?.value)
    }

    /// Returns the intersection and diagnostic statistics.
    pub fn intersection_with_stats(&self, other: &Self) -> Result<OperationReport<Self>, Error> {
        self.binary_with_stats(other, ApplyOp::Intersection)
    }

    /// Returns the sets in this family that are absent from `other`.
    pub fn difference(&self, other: &Self) -> Result<Self, Error> {
        Ok(self.difference_with_stats(other)?.value)
    }

    /// Returns the difference and diagnostic statistics.
    pub fn difference_with_stats(&self, other: &Self) -> Result<OperationReport<Self>, Error> {
        self.binary_with_stats(other, ApplyOp::Difference)
    }

    /// Returns sets that occur in exactly one of the two families.
    pub fn symmetric_difference(&self, other: &Self) -> Result<Self, Error> {
        Ok(self.symmetric_difference_with_stats(other)?.value)
    }

    /// Returns the symmetric difference and diagnostic statistics.
    pub fn symmetric_difference_with_stats(
        &self,
        other: &Self,
    ) -> Result<OperationReport<Self>, Error> {
        self.binary_with_stats(other, ApplyOp::SymmetricDifference)
    }

    /// Returns whether `set` is a member of this family.
    ///
    /// Element order and duplicate elements are normalized.
    pub fn contains(&self, set: &[VariableId]) -> Result<bool, Error> {
        let mut normalized = Vec::with_capacity(set.len());
        for element in set {
            if element.index() >= self.space.variable_count {
                return Err(Error::InvalidElement {
                    index: element.index(),
                    variable_count: self.space.variable_count,
                });
            }
            normalized.push(element.0);
        }
        normalized.sort_unstable();
        normalized.dedup();
        Ok(self.space.manager.contains(&self.root, &normalized))
    }

    /// Keeps only sets containing `element`, without removing it from the sets.
    pub fn filter_contains(&self, element: VariableId) -> Result<Self, Error> {
        self.validate_element(element)?;
        self.filter(FilterSpec::Contains(element.0))
    }

    /// Keeps only sets that do not contain `element`.
    pub fn filter_excludes(&self, element: VariableId) -> Result<Self, Error> {
        self.validate_element(element)?;
        self.filter(FilterSpec::Excludes(element.0))
    }

    /// Keeps only sets that are subsets of `elements`.
    pub fn filter_subsets_of(&self, elements: &[VariableId]) -> Result<Self, Error> {
        let normalized = self.normalize_elements(elements)?;
        self.filter(FilterSpec::Subsets(&normalized))
    }

    /// Keeps only sets that are supersets of `elements`.
    pub fn filter_supersets_of(&self, elements: &[VariableId]) -> Result<Self, Error> {
        let normalized = self.normalize_elements(elements)?;
        if normalized.is_empty() {
            return Ok(self.clone());
        }
        self.filter(FilterSpec::Supersets(&normalized))
    }

    /// Starts a cardinality filter over the number of elements in each set.
    #[must_use]
    pub fn cardinality(&self) -> CardinalityFilter<'_> {
        CardinalityFilter { family: self }
    }

    /// Returns the exact number of sets in this family.
    ///
    /// This performs query-local arbitrary-precision allocation and therefore
    /// cannot recover from allocator OOM. Use [`Self::count_index`] when the
    /// snapshot and retained-count sizes must have explicit limits.
    #[must_use]
    pub fn count(&self) -> BigUint {
        let dag = self
            .space
            .manager
            .query_snapshot(&self.root, usize::MAX)
            .expect("an unbounded snapshot cannot hit its logical node limit");
        CountIndex::from_dag(dag, None)
            .expect("an unbounded count cannot hit its logical bit limit")
            .count()
            .clone()
    }

    /// Returns the number of sets when it fits in `u128`.
    pub fn try_count_u128(&self) -> Result<u128, CountError> {
        u128::try_from(self.count()).map_err(|_| CountError::Overflow)
    }

    /// Lazily enumerates the sets in deterministic exclude-first order.
    ///
    /// Construction copies the reachable DAG, but does not count or enumerate
    /// its solutions. Consequently, methods such as [`Iterator::take`] visit
    /// only the requested solution prefix. Each item owns its element buffer
    /// and remains valid independently of this family and iterator.
    ///
    /// This iterator deliberately does not implement [`ExactSizeIterator`].
    /// Its [`Iterator::size_hint`] does not derive a `usize` upper bound by
    /// truncating a potentially much larger solution count.
    ///
    /// Unlike this method followed by [`Iterator::count`], [`Self::count`]
    /// computes the exact number on the DAG without enumerating every set.
    #[must_use]
    pub fn iter(&self) -> SolutionIterator {
        let dag = self
            .space
            .manager
            .query_snapshot(&self.root, usize::MAX)
            .expect("an unbounded snapshot cannot hit its logical node limit");
        SolutionIterator {
            traversal: SolutionTraversal::new(dag),
        }
    }

    /// Visits sets in deterministic exclude-first order using one reused buffer.
    ///
    /// The slice is valid only for the duration of its callback invocation.
    /// Returning [`ControlFlow::Break`] stops traversal immediately and returns
    /// the supplied value. Unlike [`Self::iter`], this method performs no
    /// per-solution output allocation. The manager guard is released before
    /// the first callback, so the callback may safely call APIs on this space.
    pub fn visit_solutions<B>(
        &self,
        mut visitor: impl FnMut(&[VariableId]) -> ControlFlow<B>,
    ) -> ControlFlow<B> {
        let dag = self
            .space
            .manager
            .query_snapshot(&self.root, usize::MAX)
            .expect("an unbounded snapshot cannot hit its logical node limit");
        let mut traversal = SolutionTraversal::new(dag);
        while let Some(solution) = traversal.next_slice() {
            if let ControlFlow::Break(value) = visitor(solution) {
                return ControlFlow::Break(value);
            }
        }
        ControlFlow::Continue(())
    }

    /// Builds a reusable exact-count index under explicit query limits.
    pub fn count_index(&self, limits: &QueryLimits) -> Result<CountIndex, QueryError> {
        let dag = self
            .space
            .manager
            .query_snapshot(&self.root, limits.max_snapshot_nodes)
            .map_err(|error| QueryError::LimitExceeded {
                kind: LimitKind::QueryNodes,
                limit: limits.max_snapshot_nodes,
                attempted: error.attempted,
                stats: QueryStats {
                    snapshot_nodes: error.snapshot_nodes,
                    ..QueryStats::default()
                },
            })?;
        CountIndex::from_dag(dag, Some(limits))
    }

    /// Returns whether this family contains no sets.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.space
            .manager
            .roots_equal(&self.root, &self.space.manager.empty())
    }

    /// Returns whether two families in the same space are equivalent.
    pub fn equivalent(&self, other: &Self) -> Result<bool, Error> {
        self.ensure_same_space(other)?;
        Ok(self.space.manager.roots_equal(&self.root, &other.root))
    }

    /// Returns whether every set in this family occurs in `other`.
    pub fn is_subset_of(&self, other: &Self) -> Result<bool, Error> {
        Ok(self.difference(other)?.is_empty())
    }

    fn binary_with_stats(&self, other: &Self, op: ApplyOp) -> Result<OperationReport<Self>, Error> {
        self.ensure_same_space(other)?;
        let nodes_before = self.space.manager.inner_node_count();
        match self.space.manager.apply(
            op,
            &self.root,
            &other.root,
            self.space.limits.max_operation_memo_entries,
        ) {
            Ok((root, internal)) => {
                let nodes_after = self.space.manager.inner_node_count();
                Ok(OperationReport {
                    value: Self {
                        space: Arc::clone(&self.space),
                        root,
                    },
                    stats: Self::operation_stats(nodes_before, nodes_after, internal),
                })
            }
            Err(ApplyError::MemoLimit { attempted, stats }) => {
                let nodes_after = self.space.manager.inner_node_count();
                Err(Error::LimitExceeded {
                    kind: LimitKind::OperationMemo,
                    limit: self.space.limits.max_operation_memo_entries,
                    attempted,
                    stats: Self::operation_stats(nodes_before, nodes_after, stats),
                })
            }
            Err(ApplyError::NodeLimit { stats }) => {
                let nodes_after = self.space.manager.inner_node_count();
                Err(Error::LimitExceeded {
                    kind: LimitKind::Node,
                    limit: self.space.limits.max_live_nodes,
                    attempted: self.space.limits.max_live_nodes.saturating_add(1),
                    stats: Self::operation_stats(nodes_before, nodes_after, stats),
                })
            }
        }
    }

    fn operation_stats(
        nodes_before: usize,
        nodes_after: usize,
        internal: ApplyStats,
    ) -> OperationStats {
        OperationStats {
            nodes_before,
            nodes_after,
            nodes_created: internal.nodes_created,
            peak_operation_memo_entries: internal.peak_memo_entries,
            operation_memo_hits: internal.memo_hits,
            shared_cache_hits: internal.shared_cache_hits,
            shared_cache_misses: internal.shared_cache_misses,
            cancellation_checks: 0,
        }
    }

    fn filter(&self, spec: FilterSpec<'_>) -> Result<Self, Error> {
        let nodes_before = self.space.manager.inner_node_count();
        match self.space.manager.filter(
            &self.root,
            spec,
            self.space.limits.max_operation_memo_entries,
        ) {
            Ok((root, _)) => Ok(Self {
                space: Arc::clone(&self.space),
                root,
            }),
            Err(ApplyError::MemoLimit { attempted, stats }) => {
                let nodes_after = self.space.manager.inner_node_count();
                Err(Error::LimitExceeded {
                    kind: LimitKind::OperationMemo,
                    limit: self.space.limits.max_operation_memo_entries,
                    attempted,
                    stats: Self::operation_stats(nodes_before, nodes_after, stats),
                })
            }
            Err(ApplyError::NodeLimit { stats }) => {
                let nodes_after = self.space.manager.inner_node_count();
                Err(Error::LimitExceeded {
                    kind: LimitKind::Node,
                    limit: self.space.limits.max_live_nodes,
                    attempted: self.space.limits.max_live_nodes.saturating_add(1),
                    stats: Self::operation_stats(nodes_before, nodes_after, stats),
                })
            }
        }
    }

    fn validate_element(&self, element: VariableId) -> Result<(), Error> {
        if element.index() >= self.space.variable_count {
            Err(Error::InvalidElement {
                index: element.index(),
                variable_count: self.space.variable_count,
            })
        } else {
            Ok(())
        }
    }

    fn normalize_elements(&self, elements: &[VariableId]) -> Result<Vec<u32>, Error> {
        let mut normalized = Vec::with_capacity(elements.len());
        for &element in elements {
            self.validate_element(element)?;
            normalized.push(element.0);
        }
        normalized.sort_unstable();
        normalized.dedup();
        Ok(normalized)
    }

    fn ensure_same_space(&self, other: &Self) -> Result<(), Error> {
        if Arc::ptr_eq(&self.space, &other.space) {
            Ok(())
        } else {
            Err(Error::ContextMismatch {})
        }
    }
}

impl CountIndex {
    fn from_dag(dag: QueryDag, limits: Option<&QueryLimits>) -> Result<Self, QueryError> {
        let mut counts = vec![None; dag.nodes.len() + 2];
        let mut stats = QueryStats {
            snapshot_nodes: dag.nodes.len(),
            ..QueryStats::default()
        };
        let mut work = vec![CountWork::Visit(dag.root)];

        while let Some(item) = work.pop() {
            match item {
                CountWork::Visit(reference) => {
                    if counts[reference].is_some() {
                        continue;
                    }
                    match reference {
                        QUERY_ZERO => Self::store_count(
                            reference,
                            BigUint::from(0u8),
                            &mut counts,
                            &mut stats,
                            limits,
                        )?,
                        QUERY_ONE => Self::store_count(
                            reference,
                            BigUint::from(1u8),
                            &mut counts,
                            &mut stats,
                            limits,
                        )?,
                        _ => {
                            let node = &dag.nodes[reference - 2];
                            for child in [node.hi, node.lo] {
                                if child >= 2 {
                                    debug_assert!(
                                        node.variable < dag.nodes[child - 2].variable,
                                        "ZDD children must follow their parent variable"
                                    );
                                }
                            }
                            work.push(CountWork::Finish(reference));
                            work.push(CountWork::Visit(node.hi));
                            work.push(CountWork::Visit(node.lo));
                        }
                    }
                }
                CountWork::Finish(reference) => {
                    if counts[reference].is_some() {
                        continue;
                    }
                    let node = &dag.nodes[reference - 2];
                    let value = counts[node.lo]
                        .as_ref()
                        .expect("LO count is computed before its parent")
                        + counts[node.hi]
                            .as_ref()
                            .expect("HI count is computed before its parent");
                    Self::store_count(reference, value, &mut counts, &mut stats, limits)?;
                }
            }
        }

        Ok(Self { dag, counts, stats })
    }

    fn store_count(
        reference: usize,
        value: BigUint,
        counts: &mut [Option<BigUint>],
        stats: &mut QueryStats,
        limits: Option<&QueryLimits>,
    ) -> Result<(), QueryError> {
        let bits =
            usize::try_from(value.bits()).expect("64-bit targets represent BigUint bit sizes");
        let attempted = stats.total_count_bits.saturating_add(bits);
        if let Some(limits) = limits
            && attempted > limits.max_total_count_bits
        {
            return Err(QueryError::LimitExceeded {
                kind: LimitKind::CountBits,
                limit: limits.max_total_count_bits,
                attempted,
                stats: stats.clone(),
            });
        }
        stats.total_count_bits = attempted;
        stats.max_count_bits = stats.max_count_bits.max(bits);
        counts[reference] = Some(value);
        Ok(())
    }

    /// Returns the exact number of sets represented by this index.
    #[must_use]
    pub fn count(&self) -> &BigUint {
        self.counts[self.dag.root]
            .as_ref()
            .expect("the root count is always retained")
    }

    /// Returns the resource measurements from index construction.
    #[must_use]
    pub fn stats(&self) -> &QueryStats {
        &self.stats
    }
}

impl SolutionTraversal {
    fn new(dag: QueryDag) -> Self {
        let root = dag.root;
        Self {
            dag,
            stack: vec![TraversalStep::Visit(root)],
            current: Vec::new(),
        }
    }

    fn next_slice(&mut self) -> Option<&[VariableId]> {
        while let Some(step) = self.stack.pop() {
            match step {
                TraversalStep::Visit(QUERY_ZERO) => {}
                TraversalStep::Visit(QUERY_ONE) => return Some(&self.current),
                TraversalStep::Visit(reference) => {
                    let node = &self.dag.nodes[reference - 2];
                    // LIFO order: enumerate the LO branch completely before HI.
                    self.stack.push(TraversalStep::Include {
                        reference: node.hi,
                        variable: node.variable,
                    });
                    self.stack.push(TraversalStep::Visit(node.lo));
                }
                TraversalStep::Include {
                    reference,
                    variable,
                } => {
                    self.current.push(VariableId(variable));
                    self.stack.push(TraversalStep::Remove);
                    self.stack.push(TraversalStep::Visit(reference));
                }
                TraversalStep::Remove => {
                    self.current
                        .pop()
                        .expect("every traversal removal follows an inclusion");
                }
            }
        }
        None
    }
}

impl Iterator for SolutionIterator {
    type Item = Solution;

    fn next(&mut self) -> Option<Self::Item> {
        self.traversal
            .next_slice()
            .map(|elements| Solution(elements.to_vec()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }
}

impl FusedIterator for SolutionIterator {}

impl CardinalityFilter<'_> {
    /// Keeps sets containing exactly `count` elements.
    pub fn exactly(&self, count: usize) -> Result<SetFamily, Error> {
        self.between(count..=count)
    }

    /// Keeps sets containing at most `count` elements.
    pub fn at_most(&self, count: usize) -> Result<SetFamily, Error> {
        if count >= self.family.space.variable_count {
            Ok(self.family.clone())
        } else {
            self.family.filter(FilterSpec::Cardinality {
                lower: 0,
                upper: count,
            })
        }
    }

    /// Keeps sets containing at least `count` elements.
    pub fn at_least(&self, count: usize) -> Result<SetFamily, Error> {
        if count == 0 {
            Ok(self.family.clone())
        } else if count > self.family.space.variable_count {
            Ok(SetFamily {
                space: Arc::clone(&self.family.space),
                root: self.family.space.manager.empty(),
            })
        } else {
            self.family.filter(FilterSpec::Cardinality {
                lower: count,
                upper: self.family.space.variable_count,
            })
        }
    }

    /// Keeps sets whose cardinality lies in the inclusive `range`.
    pub fn between(&self, range: std::ops::RangeInclusive<usize>) -> Result<SetFamily, Error> {
        let (start, end) = range.into_inner();
        if start > end {
            return Err(Error::InvalidRange { start, end });
        }
        if start > self.family.space.variable_count {
            return Ok(SetFamily {
                space: Arc::clone(&self.family.space),
                root: self.family.space.manager.empty(),
            });
        }
        let end = end.min(self.family.space.variable_count);
        self.family.filter(FilterSpec::Cardinality {
            lower: start,
            upper: end,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{OracleFamily, oracle_family_strategy};
    use proptest::prelude::*;

    fn small_space(variable_count: usize) -> FamilySpace {
        FamilySpace::builder(variable_count)
            .limits(Limits {
                max_live_nodes: 4_096,
                shared_cache_entries: 32,
                ..Limits::default()
            })
            .build()
            .unwrap()
    }

    fn property_test_config() -> ProptestConfig {
        let cases = std::env::var("ZDD_PROPTEST_CASES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(64);
        ProptestConfig {
            cases,
            ..ProptestConfig::default()
        }
    }

    #[test]
    fn default_limits_match_the_public_contract() {
        let limits = Limits::default();
        assert_eq!(limits.max_live_nodes, 1_000_000);
        assert_eq!(limits.max_operation_memo_entries, 1_000_000);
        assert_eq!(limits.max_frontier_states, 1_000_000);
        assert_eq!(limits.max_frontier_transitions, 10_000_000);
        assert_eq!(limits.shared_cache_entries, 262_144);

        let query_limits = QueryLimits::default();
        assert_eq!(query_limits.max_snapshot_nodes, 1_000_000);
        assert_eq!(query_limits.max_total_count_bits, 67_108_864);
    }

    #[test]
    fn constructors_distinguish_empty_and_unit() {
        let space = small_space(0);
        assert!(space.empty().is_empty());
        assert!(!space.unit().is_empty());
        assert!(
            space
                .inner
                .manager
                .roots_equal(&space.powerset().unwrap().root, &space.unit().root)
        );
    }

    #[test]
    fn exact_counts_cover_terminals_powersets_and_skipped_variables() {
        let empty_space = small_space(0);
        assert_eq!(empty_space.empty().count(), BigUint::from(0u8));
        assert_eq!(empty_space.unit().count(), BigUint::from(1u8));
        assert_eq!(empty_space.powerset().unwrap().count(), BigUint::from(1u8));

        let space = small_space(5);
        assert_eq!(space.powerset().unwrap().count(), BigUint::from(32u8));
        let last = space.variable(4).unwrap();
        let skipped = space.from_sets([vec![], vec![last]]).unwrap();
        assert_eq!(skipped.count(), BigUint::from(2u8));
        assert_eq!(skipped.try_count_u128(), Ok(2));
    }

    #[test]
    fn fixed_width_count_reports_overflow_but_exact_count_does_not() {
        let boundary_space = small_space(128);
        let boundary = boundary_space
            .powerset()
            .unwrap()
            .difference(&boundary_space.unit())
            .unwrap();
        assert_eq!(boundary.try_count_u128(), Ok(u128::MAX));

        let space = small_space(129);
        let family = space.powerset().unwrap();
        assert_eq!(family.count(), BigUint::from(1u8) << 129usize);
        assert_eq!(family.try_count_u128(), Err(CountError::Overflow));
    }

    #[test]
    fn iterator_distinguishes_terminals_and_uses_exclude_first_order() {
        let space = small_space(3);
        let a = space.variable(0).unwrap();
        let b = space.variable(1).unwrap();
        let c = space.variable(2).unwrap();

        assert_eq!(space.empty().iter().next(), None);
        assert_eq!(
            space
                .unit()
                .iter()
                .map(Solution::into_vec)
                .collect::<Vec<_>>(),
            vec![vec![]]
        );
        assert_eq!(
            space
                .powerset()
                .unwrap()
                .iter()
                .map(Solution::into_vec)
                .collect::<Vec<_>>(),
            vec![
                vec![],
                vec![c],
                vec![b],
                vec![b, c],
                vec![a],
                vec![a, c],
                vec![a, b],
                vec![a, b, c],
            ]
        );
    }

    #[test]
    fn every_small_family_is_enumerated_once_and_deterministically() {
        let space = small_space(3);
        let exclude_first = [0u64, 4, 2, 6, 1, 5, 3, 7];

        for family_mask in 0u64..=255 {
            let family = OracleFamily::from_family_mask(3, family_mask).build_in(&space);
            let actual = family
                .iter()
                .map(|solution| {
                    solution
                        .iter()
                        .fold(0u64, |set, variable| set | (1 << variable.index()))
                })
                .collect::<Vec<_>>();
            let expected = exclude_first
                .iter()
                .copied()
                .filter(|set| family_mask & (1 << set) != 0)
                .collect::<Vec<_>>();

            assert_eq!(actual, expected, "family mask {family_mask:#010b}");
            assert_eq!(
                family.iter().collect::<Vec<_>>(),
                family.iter().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn iterator_is_detached_and_owned_solutions_outlive_it() {
        let (mut iterator, a) = {
            let space = small_space(1);
            let a = space.variable(0).unwrap();
            (space.powerset().unwrap().iter(), a)
        };

        let empty = iterator.next().unwrap();
        let selected = iterator.next().unwrap();
        drop(iterator);
        assert!(empty.is_empty());
        assert_eq!(selected.as_slice(), &[a]);
    }

    #[test]
    fn large_family_take_visits_only_a_prefix_without_counting() {
        let space = small_space(129);
        let last = space.variable(128).unwrap();
        let penultimate = space.variable(127).unwrap();
        let mut iterator = space.powerset().unwrap().iter();

        assert_eq!(iterator.size_hint(), (0, None));
        assert_eq!(
            iterator
                .by_ref()
                .take(3)
                .map(Solution::into_vec)
                .collect::<Vec<_>>(),
            vec![vec![], vec![last], vec![penultimate]]
        );
        assert_eq!(iterator.size_hint(), (0, None));
    }

    #[test]
    fn visitor_reuses_traversal_state_allows_reentry_and_stops_early() {
        let space = small_space(6);
        let family = space.powerset().unwrap();
        let mut visited = Vec::new();

        let result = family.visit_solutions(|solution| {
            assert!(family.contains(solution).unwrap());
            assert_eq!(
                family.intersection(&space.unit()).unwrap().count(),
                1u8.into()
            );
            visited.push(solution.to_vec());
            if visited.len() == 5 {
                ControlFlow::Break("enough")
            } else {
                ControlFlow::Continue(())
            }
        });

        assert_eq!(result, ControlFlow::Break("enough"));
        assert_eq!(visited.len(), 5);
        assert_eq!(
            visited,
            family
                .iter()
                .take(5)
                .map(Solution::into_vec)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn visitor_handles_empty_family_and_empty_solution() {
        let space = small_space(0);
        let mut empty_calls = 0;
        assert_eq!(
            space.empty().visit_solutions::<()>(|_| {
                empty_calls += 1;
                ControlFlow::Continue(())
            }),
            ControlFlow::Continue(())
        );
        assert_eq!(empty_calls, 0);

        let mut unit_calls = 0;
        assert_eq!(
            space.unit().visit_solutions::<()>(|solution| {
                unit_calls += 1;
                assert!(solution.is_empty());
                ControlFlow::Continue(())
            }),
            ControlFlow::Continue(())
        );
        assert_eq!(unit_calls, 1);
    }

    #[test]
    fn count_index_is_detached_and_reports_logical_memory() {
        let index = {
            let space = small_space(3);
            let family = space.powerset().unwrap();
            family.count_index(&QueryLimits::default()).unwrap()
        };

        assert_eq!(index.count(), &BigUint::from(8u8));
        assert_eq!(index.stats().snapshot_nodes, 3);
        assert_eq!(index.stats().total_count_bits, 10);
        assert_eq!(index.stats().max_count_bits, 4);
    }

    #[test]
    fn count_index_enforces_node_and_count_bit_boundaries() {
        let space = small_space(3);
        let family = space.powerset().unwrap();

        let exact = QueryLimits {
            max_snapshot_nodes: 3,
            max_total_count_bits: 10,
        };
        assert_eq!(
            family.count_index(&exact).unwrap().count(),
            &BigUint::from(8u8)
        );

        let node_error = family
            .count_index(&QueryLimits {
                max_snapshot_nodes: 2,
                ..exact.clone()
            })
            .unwrap_err();
        assert!(matches!(
            node_error,
            QueryError::LimitExceeded {
                kind: LimitKind::QueryNodes,
                limit: 2,
                attempted: 3,
                stats: QueryStats {
                    snapshot_nodes: 2,
                    ..
                },
            }
        ));

        let bit_error = family
            .count_index(&QueryLimits {
                max_total_count_bits: 9,
                ..exact
            })
            .unwrap_err();
        assert!(matches!(
            bit_error,
            QueryError::LimitExceeded {
                kind: LimitKind::CountBits,
                limit: 9,
                attempted: 10,
                stats: QueryStats {
                    total_count_bits: 6,
                    max_count_bits: 3,
                    ..
                },
            }
        ));
    }

    #[test]
    fn zero_count_needs_no_count_bits_but_unit_needs_one() {
        let space = small_space(0);
        let limits = QueryLimits {
            max_snapshot_nodes: 0,
            max_total_count_bits: 0,
        };
        assert_eq!(
            space.empty().count_index(&limits).unwrap().count(),
            &BigUint::from(0u8)
        );
        assert!(matches!(
            space.unit().count_index(&limits),
            Err(QueryError::LimitExceeded {
                kind: LimitKind::CountBits,
                attempted: 1,
                ..
            })
        ));
    }

    #[test]
    fn from_sets_normalizes_elements_and_solutions() {
        let space = small_space(3);
        let a = space.variable(0).unwrap();
        let b = space.variable(1).unwrap();
        let before = space.inner.manager.inner_node_count();
        let first = space
            .from_sets([vec![b, a, a], vec![a, b], vec![]])
            .unwrap();
        let after = space.inner.manager.inner_node_count();
        let second = space.from_sets([vec![], vec![b, a]]).unwrap();

        assert!(space.inner.manager.roots_equal(&first.root, &second.root));
        assert!(after >= before);
        assert_eq!(space.inner.manager.inner_node_count(), after);
    }

    #[test]
    fn out_of_range_elements_are_rejected() {
        let left = small_space(1);
        let right = small_space(2);
        let foreign_but_out_of_range = right.variable(1).unwrap();
        assert!(matches!(
            left.from_sets([vec![foreign_but_out_of_range]]),
            Err(Error::InvalidElement { index: 1, .. })
        ));
        assert!(matches!(
            left.variable(1),
            Err(Error::InvalidElement { index: 1, .. })
        ));
    }

    #[test]
    fn initialization_checks_limits_before_creating_manager() {
        let result = FamilySpace::builder(3)
            .limits(Limits {
                max_live_nodes: 5,
                shared_cache_entries: 0,
                ..Limits::default()
            })
            .build();
        assert!(matches!(
            result,
            Err(Error::LimitExceeded {
                kind: LimitKind::Node,
                limit: 5,
                attempted: 6,
                ..
            })
        ));
    }

    #[test]
    fn construction_reports_node_capacity_without_invalidating_old_roots() {
        let space = FamilySpace::builder(2)
            .limits(Limits {
                max_live_nodes: 4,
                shared_cache_entries: 0,
                ..Limits::default()
            })
            .build()
            .unwrap();
        let stable = space.unit();
        let variable = space.variable(0).unwrap();

        let error = match space.from_sets([vec![], vec![variable]]) {
            Err(error) => error,
            Ok(_) => panic!("the manager has no capacity for another node"),
        };
        assert!(matches!(
            &error,
            Error::LimitExceeded {
                kind: LimitKind::Node,
                limit: 4,
                attempted: 5,
                ..
            }
        ));
        let Error::LimitExceeded { stats, .. } = error else {
            unreachable!();
        };
        assert_eq!(stats.nodes_before, 4);
        assert_eq!(stats.nodes_after, 4);
        assert_eq!(stats.nodes_created, 0);
        assert!(!stable.is_empty());
    }

    #[test]
    fn failed_construction_reports_nodes_created_before_the_limit() {
        let space = FamilySpace::builder(3)
            .limits(Limits {
                max_live_nodes: 7,
                shared_cache_entries: 0,
                ..Limits::default()
            })
            .build()
            .unwrap();
        let a = space.variable(0).unwrap();
        let b = space.variable(1).unwrap();
        let c = space.variable(2).unwrap();

        let error = match space.from_sets([vec![], vec![a], vec![b], vec![c]]) {
            Err(error) => error,
            Ok(_) => panic!("the result needs two nodes but only one slot is free"),
        };
        let Error::LimitExceeded { stats, .. } = error else {
            panic!("expected a node limit error");
        };
        assert_eq!(stats.nodes_before, 6);
        assert_eq!(stats.nodes_after, 7);
        assert_eq!(stats.nodes_created, 1);
    }

    #[test]
    fn cache_limit_accepts_zero_and_non_power_of_two_values() {
        for shared_cache_entries in [0, 3] {
            let space = FamilySpace::builder(2)
                .limits(Limits {
                    max_live_nodes: 16,
                    shared_cache_entries,
                    ..Limits::default()
                })
                .build()
                .unwrap();
            let variable = space.variable(0).unwrap();
            assert!(!space.from_sets([vec![variable]]).unwrap().is_empty());
        }
    }

    #[test]
    fn families_keep_the_shared_manager_alive() {
        let family = {
            let space = small_space(2);
            space.from_sets([vec![space.variable(1).unwrap()]]).unwrap()
        };
        let clone = family.clone();
        drop(family);
        assert!(!clone.is_empty());
    }

    #[test]
    fn dropping_a_deep_family_does_not_recurse_through_owned_nodes() {
        let space = FamilySpace::builder(10_000)
            .limits(Limits {
                max_live_nodes: 25_000,
                shared_cache_entries: 0,
                ..Limits::default()
            })
            .build()
            .unwrap();
        let family = space.powerset().unwrap();
        drop(space);
        drop(family);
    }

    #[test]
    fn constructing_deep_sets_does_not_recurse_through_the_zdd() {
        const VARIABLE_COUNT: usize = 10_000;
        let space = FamilySpace::builder(VARIABLE_COUNT)
            .limits(Limits {
                max_live_nodes: 35_000,
                shared_cache_entries: 0,
                ..Limits::default()
            })
            .build()
            .unwrap();
        let long: Vec<_> = (0..VARIABLE_COUNT)
            .map(|index| space.variable(index).unwrap())
            .collect();
        let prefix = long[..VARIABLE_COUNT - 1].to_vec();

        let family = space.from_sets([long, prefix]).unwrap();
        assert_eq!(family.count(), BigUint::from(2u8));
        assert!(!family.is_empty());
        let result = family.intersection(&space.powerset().unwrap()).unwrap();
        assert!(result.equivalent(&family).unwrap());
    }

    #[test]
    fn all_three_variable_families_match_the_explicit_oracle() {
        let space = FamilySpace::builder(3)
            .limits(Limits {
                max_live_nodes: 8_192,
                max_operation_memo_entries: 1_024,
                shared_cache_entries: 17,
                ..Limits::default()
            })
            .build()
            .unwrap();
        let oracles = (0u64..=255)
            .map(|mask| OracleFamily::from_family_mask(3, mask))
            .collect::<Vec<_>>();
        let families = oracles
            .iter()
            .map(|oracle| oracle.build_in(&space))
            .collect::<Vec<_>>();

        for left in 0usize..=255 {
            for right in 0usize..=255 {
                let union = families[left].union(&families[right]).unwrap();
                let intersection = families[left].intersection(&families[right]).unwrap();
                let difference = families[left].difference(&families[right]).unwrap();
                let symmetric_difference = families[left]
                    .symmetric_difference(&families[right])
                    .unwrap();

                oracles[left]
                    .union(&oracles[right])
                    .assert_matches(&space, &union);
                oracles[left]
                    .intersection(&oracles[right])
                    .assert_matches(&space, &intersection);
                oracles[left]
                    .difference(&oracles[right])
                    .assert_matches(&space, &difference);
                oracles[left]
                    .symmetric_difference(&oracles[right])
                    .assert_matches(&space, &symmetric_difference);
                assert_eq!(
                    families[left].is_subset_of(&families[right]).unwrap(),
                    oracles[left].is_subset_of(&oracles[right])
                );
            }
        }

        for (oracle, family) in oracles.iter().zip(&families) {
            oracle.assert_matches(&space, family);
        }
    }

    #[test]
    fn all_four_variable_families_pass_unary_and_normalization_checks() {
        let space = FamilySpace::builder(4)
            .limits(Limits {
                max_live_nodes: 16_384,
                shared_cache_entries: 0,
                ..Limits::default()
            })
            .build()
            .unwrap();

        for mask in 0u64..=u16::MAX as u64 {
            let oracle = OracleFamily::from_family_mask(4, mask);
            let family = oracle.build_in_with_normalization_noise(&space);
            oracle.assert_matches(&space, &family);
            assert_eq!(family.count(), BigUint::from(oracle.count()));
            assert!(family.equivalent(&oracle.build_in(&space)).unwrap());
        }
    }

    proptest! {
        #![proptest_config(property_test_config())]

        #[test]
        fn family_operations_obey_algebra_and_preserve_inputs(
            left_oracle in oracle_family_strategy(4),
            middle_oracle in oracle_family_strategy(4),
            right_oracle in oracle_family_strategy(4),
        ) {
            let space = small_space(4);
            let left = left_oracle.build_in_with_normalization_noise(&space);
            let middle = middle_oracle.build_in(&space);
            let right = right_oracle.build_in_with_normalization_noise(&space);

            let union_lr = left.union(&right).unwrap();
            let union_rl = right.union(&left).unwrap();
            prop_assert!(union_lr.equivalent(&union_rl).unwrap());
            prop_assert!(left.union(&left).unwrap().equivalent(&left).unwrap());
            prop_assert!(left.union(&middle).unwrap().union(&right).unwrap()
                .equivalent(&left.union(&middle.union(&right).unwrap()).unwrap()).unwrap());

            let intersection_lr = left.intersection(&right).unwrap();
            let intersection_rl = right.intersection(&left).unwrap();
            prop_assert!(intersection_lr.equivalent(&intersection_rl).unwrap());
            prop_assert!(left.intersection(&left).unwrap().equivalent(&left).unwrap());
            prop_assert!(left.intersection(&middle).unwrap().intersection(&right).unwrap()
                .equivalent(&left.intersection(&middle.intersection(&right).unwrap()).unwrap()).unwrap());

            prop_assert!(left.difference(&left).unwrap().is_empty());
            prop_assert!(left.symmetric_difference(&left).unwrap().is_empty());

            left_oracle.union(&right_oracle).assert_matches(&space, &union_lr);
            left_oracle.intersection(&right_oracle).assert_matches(&space, &intersection_lr);
            left_oracle.difference(&right_oracle)
                .assert_matches(&space, &left.difference(&right).unwrap());
            left_oracle.symmetric_difference(&right_oracle)
                .assert_matches(&space, &left.symmetric_difference(&right).unwrap());

            // Every operation above must leave its immutable inputs unchanged.
            left_oracle.assert_matches(&space, &left);
            middle_oracle.assert_matches(&space, &middle);
            right_oracle.assert_matches(&space, &right);
        }
    }

    #[test]
    fn cross_space_binary_operations_and_comparisons_are_rejected() {
        let left = small_space(1).unit();
        let right = small_space(1).unit();

        assert!(matches!(
            left.union(&right),
            Err(Error::ContextMismatch { .. })
        ));
        assert!(matches!(
            left.intersection(&right),
            Err(Error::ContextMismatch { .. })
        ));
        assert!(matches!(
            left.difference(&right),
            Err(Error::ContextMismatch { .. })
        ));
        assert!(matches!(
            left.symmetric_difference(&right),
            Err(Error::ContextMismatch { .. })
        ));
        assert!(matches!(
            left.equivalent(&right),
            Err(Error::ContextMismatch { .. })
        ));
        assert!(matches!(
            left.is_subset_of(&right),
            Err(Error::ContextMismatch { .. })
        ));
    }

    #[test]
    fn contains_normalizes_order_and_duplicates() {
        let space = small_space(3);
        let a = space.variable(0).unwrap();
        let c = space.variable(2).unwrap();
        let family = space.from_sets([vec![a, c]]).unwrap();

        assert!(family.contains(&[c, a, c]).unwrap());
        assert!(!family.contains(&[a]).unwrap());
    }

    #[test]
    fn all_filters_match_the_explicit_oracle_and_partition_families() {
        let space = small_space(3);
        let variables = (0..3)
            .map(|index| space.variable(index).unwrap())
            .collect::<Vec<_>>();

        for mask in 0u64..=255 {
            let oracle = OracleFamily::from_family_mask(3, mask);
            let family = oracle.build_in(&space);

            for (variable, &element) in variables.iter().enumerate() {
                let containing = family.filter_contains(element).unwrap();
                let excluding = family.filter_excludes(element).unwrap();
                oracle
                    .filter_contains(variable)
                    .assert_matches(&space, &containing);
                oracle
                    .filter_excludes(variable)
                    .assert_matches(&space, &excluding);
                assert!(containing.intersection(&excluding).unwrap().is_empty());
                assert!(
                    containing
                        .union(&excluding)
                        .unwrap()
                        .equivalent(&family)
                        .unwrap()
                );
            }

            for target in 0u64..8 {
                let elements = (0..3)
                    .rev()
                    .filter(|variable| target & (1 << variable) != 0)
                    .flat_map(|variable| [variables[variable], variables[variable]])
                    .collect::<Vec<_>>();
                oracle
                    .filter_subsets_of(target)
                    .assert_matches(&space, &family.filter_subsets_of(&elements).unwrap());
                oracle
                    .filter_supersets_of(target)
                    .assert_matches(&space, &family.filter_supersets_of(&elements).unwrap());
            }

            for lower in 0..=4 {
                for upper in lower..=4 {
                    oracle.filter_cardinality(lower, upper).assert_matches(
                        &space,
                        &family.cardinality().between(lower..=upper).unwrap(),
                    );
                }
            }
        }
    }

    #[test]
    fn filter_boundaries_and_invalid_inputs_are_explicit() {
        let space = small_space(2);
        let a = space.variable(0).unwrap();
        let family = space.powerset().unwrap();

        assert!(family.cardinality().exactly(3).unwrap().is_empty());
        assert!(
            family
                .cardinality()
                .at_least(0)
                .unwrap()
                .equivalent(&family)
                .unwrap()
        );
        assert!(
            family
                .cardinality()
                .at_most(usize::MAX)
                .unwrap()
                .equivalent(&family)
                .unwrap()
        );
        assert!(
            family
                .cardinality()
                .at_least(usize::MAX)
                .unwrap()
                .is_empty()
        );
        let reversed_start = 2;
        let reversed_end = 1;
        assert!(matches!(
            family.cardinality().between(reversed_start..=reversed_end),
            Err(Error::InvalidRange { start: 2, end: 1 })
        ));
        assert!(
            family
                .filter_supersets_of(&[])
                .unwrap()
                .equivalent(&family)
                .unwrap()
        );
        let only_empty = family.filter_subsets_of(&[]).unwrap();
        assert!(only_empty.contains(&[]).unwrap());
        assert!(!only_empty.contains(&[a]).unwrap());

        let larger_space = small_space(3);
        let out_of_range = larger_space.variable(2).unwrap();
        assert!(matches!(
            family.filter_contains(out_of_range),
            Err(Error::InvalidElement { index: 2, .. })
        ));
        assert!(matches!(
            family.filter_subsets_of(&[out_of_range]),
            Err(Error::InvalidElement { index: 2, .. })
        ));
    }

    #[test]
    fn filtering_composes_with_construction_intersection_and_queries() {
        let space = small_space(4);
        let variables = (0..4)
            .map(|index| space.variable(index).unwrap())
            .collect::<Vec<_>>();
        let left = space.powerset().unwrap();
        let right = space
            .from_sets([
                vec![variables[0], variables[1]],
                vec![variables[0], variables[2], variables[3]],
                vec![variables[1], variables[2]],
            ])
            .unwrap();

        let result = left
            .intersection(&right)
            .unwrap()
            .filter_contains(variables[0])
            .unwrap()
            .cardinality()
            .exactly(2)
            .unwrap();
        assert!(result.contains(&[variables[0], variables[1]]).unwrap());
        assert!(
            !result
                .contains(&[variables[0], variables[2], variables[3]])
                .unwrap()
        );
        assert!(!result.contains(&[variables[1], variables[2]]).unwrap());
    }

    #[test]
    fn filters_honor_operation_memo_limits_without_invalidating_inputs() {
        let space = FamilySpace::builder(2)
            .limits(Limits {
                max_live_nodes: 64,
                max_operation_memo_entries: 0,
                shared_cache_entries: 0,
                ..Limits::default()
            })
            .build()
            .unwrap();
        let family = space.powerset().unwrap();
        let a = space.variable(0).unwrap();

        assert!(matches!(
            family.filter_contains(a),
            Err(Error::LimitExceeded {
                kind: LimitKind::OperationMemo,
                limit: 0,
                attempted: 1,
                ..
            })
        ));
        assert!(family.contains(&[]).unwrap());
        assert!(family.contains(&[a]).unwrap());
    }

    #[test]
    fn filtering_a_deep_zdd_does_not_use_the_call_stack() {
        const VARIABLE_COUNT: usize = 2_000;
        let space = FamilySpace::builder(VARIABLE_COUNT)
            .limits(Limits {
                max_live_nodes: 40_000,
                max_operation_memo_entries: 20_000,
                shared_cache_entries: 0,
                ..Limits::default()
            })
            .build()
            .unwrap();
        let variables = (0..VARIABLE_COUNT)
            .map(|index| space.variable(index).unwrap())
            .collect::<Vec<_>>();
        let family = space.from_sets([variables.clone()]).unwrap();

        let result = family
            .filter_contains(variables[VARIABLE_COUNT - 1])
            .unwrap()
            .cardinality()
            .exactly(VARIABLE_COUNT)
            .unwrap();
        assert!(result.contains(&variables).unwrap());
    }

    #[test]
    fn operation_memo_limit_is_an_explicit_error() {
        let space = FamilySpace::builder(2)
            .limits(Limits {
                max_live_nodes: 64,
                max_operation_memo_entries: 0,
                shared_cache_entries: 0,
                ..Limits::default()
            })
            .build()
            .unwrap();
        let a = space.from_sets([vec![space.variable(0).unwrap()]]).unwrap();
        let b = space.from_sets([vec![space.variable(1).unwrap()]]).unwrap();

        assert!(matches!(
            a.union(&b),
            Err(Error::LimitExceeded {
                kind: LimitKind::OperationMemo,
                limit: 0,
                attempted: 1,
                ..
            })
        ));
        assert!(a.contains(&[space.variable(0).unwrap()]).unwrap());
        assert!(b.contains(&[space.variable(1).unwrap()]).unwrap());
    }

    #[test]
    fn cache_eviction_and_cache_disable_do_not_change_results() {
        for cache_capacity in [0, 1] {
            let space = FamilySpace::builder(3)
                .limits(Limits {
                    max_live_nodes: 256,
                    shared_cache_entries: cache_capacity,
                    ..Limits::default()
                })
                .build()
                .unwrap();
            let left_oracle = OracleFamily::from_family_mask(3, 0b1010_1010);
            let right_oracle = OracleFamily::from_family_mask(3, 0b1100_1100);
            let left = left_oracle.build_in(&space);
            let right = right_oracle.build_in(&space);

            left_oracle
                .union(&right_oracle)
                .assert_matches(&space, &left.union(&right).unwrap());
            left_oracle
                .intersection(&right_oracle)
                .assert_matches(&space, &left.intersection(&right).unwrap());
            left_oracle
                .difference(&right_oracle)
                .assert_matches(&space, &left.difference(&right).unwrap());
            left_oracle
                .symmetric_difference(&right_oracle)
                .assert_matches(&space, &left.symmetric_difference(&right).unwrap());

            let stats = space.stats();
            assert!(stats.shared_cache_entries <= cache_capacity);
            if cache_capacity == 0 {
                assert_eq!(stats.shared_cache_evictions, 0);
            } else {
                assert!(stats.shared_cache_evictions > 0);
            }
        }
    }
}
