use super::*;

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

    #[cfg(feature = "graph")]
    pub(crate) fn limits(&self) -> &Limits {
        &self.inner.limits
    }

    #[cfg(feature = "graph")]
    pub(crate) fn make_decision_node(
        &self,
        variable: VariableId,
        hi: &SetFamily,
        lo: &SetFamily,
    ) -> Result<(SetFamily, bool), ()> {
        debug_assert!(Arc::ptr_eq(&self.inner, &hi.space));
        debug_assert!(Arc::ptr_eq(&self.inner, &lo.space));
        let (root, created) = self
            .inner
            .manager
            .make_node(variable.0, &hi.root, &lo.root)?;
        Ok((self.family(root), created))
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
