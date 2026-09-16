//! Public, graph-independent set-family API.

use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

use crate::zdd::{CreateError, Root, ZddManager};

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

/// The resource whose limit was exceeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LimitKind {
    /// Live nonterminal nodes in the shared manager.
    Node,
}

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

        let manager =
            ZddManager::new(self.variable_count, self.limits.max_live_nodes).map_err(|error| {
                match error {
                    CreateError::TooManyVariables | CreateError::NodeCapacityTooLarge => {
                        Error::CapacityOverflow
                    }
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
    /// Returns whether this family contains no sets.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.space
            .manager
            .roots_equal(&self.root, &self.space.manager.empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn default_limits_match_the_public_contract() {
        let limits = Limits::default();
        assert_eq!(limits.max_live_nodes, 1_000_000);
        assert_eq!(limits.max_operation_memo_entries, 1_000_000);
        assert_eq!(limits.max_frontier_states, 1_000_000);
        assert_eq!(limits.max_frontier_transitions, 10_000_000);
        assert_eq!(limits.shared_cache_entries, 262_144);
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
        assert!(!family.is_empty());
    }
}
