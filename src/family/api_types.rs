use super::*;

use std::sync::atomic::{AtomicBool, Ordering};

/// An element identifier in a [`FamilySpace`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariableId(pub(super) u32);

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
    /// Distinct canonical states retained in one frontier layer.
    FrontierStates,
    /// Include/exclude transitions attempted by one frontier build.
    FrontierTransitions,
    /// Distinct nonterminal nodes copied into a query-local DAG.
    QueryNodes,
    /// Logical bits in all exact partial counts retained by a query.
    CountBits,
}

/// A cloneable handle for cooperative cancellation of bounded operations.
///
/// Cancellation is monotonic: after [`Self::cancel`] is called, every clone
/// remains cancelled. Operations observe cancellation between work units; the
/// handle does not interrupt a user callback that is already running.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Creates a token in the non-cancelled state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation for every clone of this token.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
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
    pub(super) fn node_construction(
        nodes_before: usize,
        nodes_after: usize,
        nodes_created: usize,
    ) -> Self {
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
    /// Source and destination universes have different sizes.
    #[non_exhaustive]
    UniverseSizeMismatch {
        /// Number of variables in the source universe.
        source: usize,
        /// Number of variables in the destination universe.
        destination: usize,
    },
    /// A variable map does not cover the complete source universe.
    #[non_exhaustive]
    InvalidVariableMapLength {
        /// Required number of source entries.
        expected: usize,
        /// Number of entries that were supplied.
        actual: usize,
    },
    /// A mapped destination variable is outside the destination universe.
    #[non_exhaustive]
    InvalidMappedVariable {
        /// Source variable position whose mapping was rejected.
        source_index: usize,
        /// Rejected destination variable position.
        destination_index: usize,
        /// Number of variables in the destination universe.
        destination_variable_count: usize,
    },
    /// Two source variables map to the same destination variable.
    #[non_exhaustive]
    DuplicateMappedVariable {
        /// Duplicated destination variable position.
        destination_index: usize,
        /// First source position mapped there.
        first_source_index: usize,
        /// Later source position mapped there.
        duplicate_source_index: usize,
    },
    /// A variable map does not preserve the fixed variable order.
    #[non_exhaustive]
    OrderMismatch {},
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
            Self::UniverseSizeMismatch {
                source,
                destination,
            } => write!(
                formatter,
                "source universe has {source} variables but destination has {destination}"
            ),
            Self::InvalidVariableMapLength { expected, actual } => write!(
                formatter,
                "variable map has {actual} entries but the source universe has {expected} variables"
            ),
            Self::InvalidMappedVariable {
                source_index,
                destination_index,
                destination_variable_count,
            } => write!(
                formatter,
                "source variable {source_index} maps to destination variable {destination_index}, outside a universe of {destination_variable_count} variables"
            ),
            Self::DuplicateMappedVariable {
                destination_index,
                first_source_index,
                duplicate_source_index,
            } => write!(
                formatter,
                "source variables {first_source_index} and {duplicate_source_index} both map to destination variable {destination_index}"
            ),
            Self::OrderMismatch {} => {
                formatter.write_str("variable map does not preserve variable order")
            }
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
