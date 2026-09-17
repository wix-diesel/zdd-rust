use super::*;

impl ZddManager {
    pub(crate) fn stats(&self) -> ManagerStats {
        let cache = self
            .shared_cache
            .lock()
            .expect("shared cache lock poisoned");
        ManagerStats {
            peak_live_nodes: self.peak_live_nodes.load(Ordering::Relaxed),
            nodes_created: self.nodes_created.load(Ordering::Relaxed),
            shared_cache_entries: cache.entries.len(),
            shared_cache_hits: cache.hits,
            shared_cache_misses: cache.misses,
            shared_cache_evictions: cache.evictions,
            gc_count: self
                .manager
                .with_manager_shared(|backend| backend.gc_count()),
        }
    }

    /// Copies one reachable DAG into traversal-local IDs while holding one
    /// backend read guard. No backend node identity escapes this method.
    #[allow(clippy::mutable_key_type)]
    pub(crate) fn query_snapshot(
        &self,
        root: &Root,
        max_nodes: usize,
    ) -> Result<QueryDag, SnapshotLimitError> {
        let zero = self.empty();
        let one = self.unit();

        self.manager.with_manager_shared(|backend| {
            fn reference(
                root: Root,
                zero: &Root,
                one: &Root,
                max_nodes: usize,
                roots: &mut Vec<Root>,
                ids: &mut HashMap<Root, usize>,
                nodes: &mut Vec<Option<QueryNode>>,
            ) -> Result<usize, SnapshotLimitError> {
                if root == *zero {
                    return Ok(QUERY_ZERO);
                }
                if root == *one {
                    return Ok(QUERY_ONE);
                }
                if let Some(&id) = ids.get(&root) {
                    return Ok(id);
                }
                if nodes.len() == max_nodes {
                    return Err(SnapshotLimitError {
                        attempted: nodes.len().saturating_add(1),
                        snapshot_nodes: nodes.len(),
                    });
                }

                let id = nodes.len() + 2;
                ids.insert(root.clone(), id);
                roots.push(root);
                nodes.push(None);
                Ok(id)
            }

            let mut roots = Vec::new();
            let mut ids = HashMap::new();
            let mut nodes = Vec::new();
            let root = reference(
                root.clone(),
                &zero,
                &one,
                max_nodes,
                &mut roots,
                &mut ids,
                &mut nodes,
            )?;

            let mut index = 0;
            while index < roots.len() {
                let Node::Inner(node) = backend.get_node(roots[index].0.as_edge(backend)) else {
                    unreachable!("query terminals use reserved local IDs");
                };
                let variable = backend.level_to_var(node.level());
                let mut children = node.children();
                let hi = Root(ZBDDFunction::from_edge(
                    backend,
                    backend.clone_edge(&children.next().unwrap()),
                ));
                let lo = Root(ZBDDFunction::from_edge(
                    backend,
                    backend.clone_edge(&children.next().unwrap()),
                ));
                let hi = reference(hi, &zero, &one, max_nodes, &mut roots, &mut ids, &mut nodes)?;
                let lo = reference(lo, &zero, &one, max_nodes, &mut roots, &mut ids, &mut nodes)?;
                nodes[index] = Some(QueryNode { variable, hi, lo });
                index += 1;
            }

            Ok(QueryDag {
                root,
                nodes: nodes
                    .into_iter()
                    .map(|node| node.expect("every discovered query node is visited"))
                    .collect(),
            })
        })
    }
}
