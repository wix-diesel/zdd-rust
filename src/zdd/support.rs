use super::*;

impl ZddManager {
    pub(super) fn key(op: ApplyOp, mut left: Root, mut right: Root) -> ApplyKey {
        if op != ApplyOp::Difference && right < left {
            std::mem::swap(&mut left, &mut right);
        }
        ApplyKey { op, left, right }
    }

    pub(super) fn terminal_result(key: &ApplyKey, zero: &Root) -> Option<Root> {
        let left_zero = key.left == *zero;
        let right_zero = key.right == *zero;
        let equal = key.left == key.right;
        match key.op {
            ApplyOp::Union => {
                if left_zero {
                    Some(key.right.clone())
                } else if right_zero || equal {
                    Some(key.left.clone())
                } else {
                    None
                }
            }
            ApplyOp::Intersection => {
                if left_zero || right_zero {
                    Some(zero.clone())
                } else if equal {
                    Some(key.left.clone())
                } else {
                    None
                }
            }
            ApplyOp::Difference => {
                if left_zero || equal {
                    Some(zero.clone())
                } else if right_zero {
                    Some(key.left.clone())
                } else {
                    None
                }
            }
            ApplyOp::SymmetricDifference => {
                if equal {
                    Some(zero.clone())
                } else if left_zero {
                    Some(key.right.clone())
                } else if right_zero {
                    Some(key.left.clone())
                } else {
                    None
                }
            }
        }
    }

    pub(super) fn view(&self, root: &Root) -> RootView {
        self.manager.with_manager_shared(|backend| {
            match backend.get_node(root.0.as_edge(backend)) {
                Node::Terminal(_) => RootView::Terminal,
                Node::Inner(node) => {
                    let variable = backend.level_to_var(node.level());
                    let mut children = node.children();
                    let hi = ZBDDFunction::from_edge(
                        backend,
                        backend.clone_edge(&children.next().unwrap()),
                    );
                    let lo = ZBDDFunction::from_edge(
                        backend,
                        backend.clone_edge(&children.next().unwrap()),
                    );
                    RootView::Node {
                        variable,
                        hi: Root(hi),
                        lo: Root(lo),
                    }
                }
            }
        })
    }

    pub(super) fn cofactors_at(
        view: RootView,
        variable: u32,
        root: &Root,
        zero: &Root,
    ) -> (Root, Root) {
        match view {
            RootView::Node {
                variable: own,
                hi,
                lo,
            } if own == variable => (hi, lo),
            RootView::Node { .. } | RootView::Terminal => (zero.clone(), root.clone()),
        }
    }

    pub(super) fn make_node(
        &self,
        variable: u32,
        hi: &Root,
        lo: &Root,
    ) -> Result<(Root, bool), ()> {
        self.manager.with_manager_shared(|backend| {
            let before = backend.num_inner_nodes();
            let gc_before = backend.gc_count();
            let hi = backend.clone_edge(hi.0.as_edge(backend));
            let lo = backend.clone_edge(lo.0.as_edge(backend));
            let edge = oxidd::zbdd::make_node(
                backend,
                self.variables[variable as usize].as_edge(backend),
                hi,
                lo,
            )
            .map_err(|_| ())?;
            let created = backend.gc_count() != gc_before || backend.num_inner_nodes() > before;
            if created {
                self.record_node_created(backend.num_inner_nodes());
            }
            Ok((Root(ZBDDFunction::from_edge(backend, edge)), created))
        })
    }

    pub(super) fn record_node_created(&self, live_nodes: usize) {
        self.nodes_created.fetch_add(1, Ordering::Relaxed);
        self.peak_live_nodes
            .fetch_max(live_nodes, Ordering::Relaxed);
    }

    pub(super) fn shared_cache_get(&self, key: &ApplyKey, stats: &mut ApplyStats) -> Option<Root> {
        let mut cache = self
            .shared_cache
            .lock()
            .expect("shared cache lock poisoned");
        if cache.capacity == 0 {
            return None;
        }
        if let Some(result) = cache.entries.get(key).cloned() {
            cache.hits += 1;
            stats.shared_cache_hits += 1;
            Some(result)
        } else {
            cache.misses += 1;
            stats.shared_cache_misses += 1;
            None
        }
    }

    pub(super) fn shared_cache_insert(&self, key: ApplyKey, result: Root) {
        let mut cache = self
            .shared_cache
            .lock()
            .expect("shared cache lock poisoned");
        if cache.capacity == 0 || cache.entries.contains_key(&key) {
            return;
        }
        if cache.entries.len() == cache.capacity
            && let Some(evicted) = cache.insertion_order.pop_front()
        {
            cache.entries.remove(&evicted);
            cache.evictions += 1;
        }
        cache.insertion_order.push_back(key.clone());
        cache.entries.insert(key, result);
    }

    pub(crate) fn inner_node_count(&self) -> usize {
        self.manager
            .with_manager_shared(|backend| backend.num_inner_nodes())
    }

    #[cfg(test)]
    pub(super) fn count(&self, root: &Root) -> u128 {
        let mut cache = SatCountCache::<u128, std::collections::hash_map::RandomState>::default();
        root.0.sat_count(self.variables.len() as u32, &mut cache)
    }

    /// Serializes a reachable DAG using deterministic traversal-local IDs.
    /// Backend node IDs and hash-table iteration order are never observed.
    #[cfg(test)]
    pub(crate) fn normalized_snapshot(&self, root: &Root) -> String {
        fn reference(
            manager: &ZddManager,
            root: Root,
            zero: &Root,
            one: &Root,
            nodes: &mut Vec<Root>,
        ) -> String {
            if manager.roots_equal(&root, zero) {
                return "ZERO".to_owned();
            }
            if manager.roots_equal(&root, one) {
                return "ONE".to_owned();
            }
            if let Some(index) = nodes
                .iter()
                .position(|known| manager.roots_equal(known, &root))
            {
                return format!("n{index}");
            }
            let index = nodes.len();
            nodes.push(root);
            format!("n{index}")
        }

        let zero = self.empty();
        let one = self.unit();
        let mut nodes = Vec::new();
        let root_reference = reference(self, root.clone(), &zero, &one, &mut nodes);
        let mut lines = vec![format!("root={root_reference}")];
        let mut index = 0;
        while index < nodes.len() {
            let RootView::Node { variable, hi, lo } = self.view(&nodes[index]) else {
                unreachable!("terminals are represented by names, not local node IDs");
            };
            let hi = reference(self, hi, &zero, &one, &mut nodes);
            let lo = reference(self, lo, &zero, &one, &mut nodes);
            lines.push(format!("n{index}: variable={variable}, hi={hi}, lo={lo}"));
            index += 1;
        }
        lines.join("\n")
    }
}
