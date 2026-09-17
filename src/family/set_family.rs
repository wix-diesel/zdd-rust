use super::*;

impl SetFamily {
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

    pub(super) fn binary_with_stats(
        &self,
        other: &Self,
        op: ApplyOp,
    ) -> Result<OperationReport<Self>, Error> {
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

    pub(super) fn filter(&self, spec: FilterSpec<'_>) -> Result<Self, Error> {
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

    pub(super) fn validate_element(&self, element: VariableId) -> Result<(), Error> {
        if element.index() >= self.space.variable_count {
            Err(Error::InvalidElement {
                index: element.index(),
                variable_count: self.space.variable_count,
            })
        } else {
            Ok(())
        }
    }

    pub(super) fn normalize_elements(&self, elements: &[VariableId]) -> Result<Vec<u32>, Error> {
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
