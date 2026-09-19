use std::error::Error as StdError;
use std::fmt;

use crate::{GraphError, LimitKind, OperationStats};

/// Statistics collected while building a family with frontier DP.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BuildStats {
    /// ZDD-manager work performed by the backward construction phase.
    pub operation: OperationStats,
    /// Edge layers completely expanded by the forward DP.
    pub layers_processed: usize,
    /// Canonical states in the most recently completed layer.
    pub current_frontier_states: usize,
    /// Largest number of canonical states retained in one layer.
    pub peak_frontier_states: usize,
    /// User include/exclude transitions invoked by this build.
    pub transitions_attempted: usize,
    /// Completed branch records retained in the all-layer transition tape.
    pub transition_tape_entries: usize,
    /// Branches rejected by the problem.
    pub branches_rejected: usize,
    /// Kept states merged with an equal canonical state in the same layer.
    pub states_merged: usize,
}

/// A successful frontier build and its diagnostic statistics.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct BuildReport<T> {
    /// The complete accepted family.
    pub value: T,
    /// Work performed by the build.
    pub stats: BuildStats,
}

/// Error returned by a frontier build.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BuildError<E> {
    /// The fixed graph/edge-order plan could not be constructed.
    #[non_exhaustive]
    Graph {
        /// Underlying graph input or mapping error.
        source: GraphError,
        /// Work completed before the error.
        stats: Box<BuildStats>,
    },
    /// A configured resource limit was exceeded.
    #[non_exhaustive]
    LimitExceeded {
        /// Resource that could not be added.
        kind: LimitKind,
        /// Configured maximum.
        limit: usize,
        /// Value the build attempted to reach.
        attempted: usize,
        /// Work completed before the error.
        stats: Box<BuildStats>,
    },
    /// Cooperative cancellation was observed between work units.
    #[non_exhaustive]
    Cancelled {
        /// Work completed before cancellation.
        stats: Box<BuildStats>,
    },
    /// A user problem callback returned an error.
    #[non_exhaustive]
    Problem {
        /// Original problem-specific error value.
        source: E,
        /// Work completed before the error.
        stats: Box<BuildStats>,
    },
}

impl<E> BuildError<E> {
    /// Returns statistics captured at the failure point.
    #[must_use]
    pub fn stats(&self) -> &BuildStats {
        match self {
            Self::Graph { stats, .. }
            | Self::LimitExceeded { stats, .. }
            | Self::Cancelled { stats }
            | Self::Problem { stats, .. } => stats.as_ref(),
        }
    }
}

impl<E> fmt::Display for BuildError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Graph { source, .. } => source.fmt(formatter),
            Self::LimitExceeded {
                kind,
                limit,
                attempted,
                ..
            } => write!(
                formatter,
                "{kind:?} limit {limit} exceeded while attempting to use {attempted}"
            ),
            Self::Cancelled { .. } => formatter.write_str("frontier build was cancelled"),
            Self::Problem { .. } => formatter.write_str("frontier problem returned an error"),
        }
    }
}

impl<E: StdError + 'static> StdError for BuildError<E> {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Graph { source, .. } => Some(source),
            Self::Problem { source, .. } => Some(source),
            Self::LimitExceeded { .. } | Self::Cancelled { .. } => None,
        }
    }
}
