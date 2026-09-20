use super::*;

impl ZddManager {
    /// Copies several reachable roots into one local DAG. Discovering all roots
    /// in one pass preserves sharing between them without exposing backend IDs.
    #[allow(clippy::mutable_key_type)]
    pub(crate) fn transfer_snapshot(&self, roots: &[&Root]) -> TransferDag {
        let zero = self.empty();
        let one = self.unit();

        self.manager.with_manager_shared(|backend| {
            fn reference(
                root: Root,
                zero: &Root,
                one: &Root,
                pending: &mut Vec<Root>,
                ids: &mut HashMap<Root, usize>,
                nodes: &mut Vec<Option<QueryNode>>,
            ) -> usize {
                if root == *zero {
                    return QUERY_ZERO;
                }
                if root == *one {
                    return QUERY_ONE;
                }
                if let Some(&id) = ids.get(&root) {
                    return id;
                }

                let id = nodes.len() + 2;
                ids.insert(root.clone(), id);
                pending.push(root);
                nodes.push(None);
                id
            }

            let mut pending = Vec::new();
            let mut ids = HashMap::new();
            let mut nodes = Vec::new();
            let roots = roots
                .iter()
                .map(|root| {
                    reference(
                        (*root).clone(),
                        &zero,
                        &one,
                        &mut pending,
                        &mut ids,
                        &mut nodes,
                    )
                })
                .collect();

            let mut index = 0;
            while index < pending.len() {
                let Node::Inner(node) = backend.get_node(pending[index].0.as_edge(backend)) else {
                    unreachable!("transfer terminals use reserved local IDs");
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
                let hi = reference(hi, &zero, &one, &mut pending, &mut ids, &mut nodes);
                let lo = reference(lo, &zero, &one, &mut pending, &mut ids, &mut nodes);
                nodes[index] = Some(QueryNode { variable, hi, lo });
                index += 1;
            }

            TransferDag {
                roots,
                nodes: nodes
                    .into_iter()
                    .map(|node| node.expect("every discovered transfer node is visited"))
                    .collect(),
            }
        })
    }

    /// Rebuilds a local DAG using an explicit source-to-destination variable map.
    /// The explicit work stack avoids recursion even when roots are nested.
    pub(crate) fn import_dag(
        &self,
        dag: &TransferDag,
        variable_map: &[u32],
    ) -> Result<(Vec<Root>, usize), BuildError> {
        self.manager.with_manager_shared(|backend| {
            let mut built: Vec<Option<ZBDDFunction>> = (0..dag.nodes.len()).map(|_| None).collect();
            let mut state = vec![0u8; dag.nodes.len()];
            let mut work = dag.roots.clone();
            let mut nodes_created = 0;

            while let Some(reference) = work.pop() {
                if reference < 2 {
                    continue;
                }
                let index = reference - 2;
                match state[index] {
                    0 => {
                        state[index] = 1;
                        work.push(reference);
                        let node = &dag.nodes[index];
                        if node.lo >= 2 && state[node.lo - 2] == 0 {
                            work.push(node.lo);
                        }
                        if node.hi >= 2 && state[node.hi - 2] == 0 {
                            work.push(node.hi);
                        }
                    }
                    1 => {
                        let node = &dag.nodes[index];
                        let branch = |reference: usize| match reference {
                            QUERY_ZERO => ZBDDFunction::empty_edge(backend),
                            QUERY_ONE => ZBDDFunction::base_edge(backend),
                            _ => backend.clone_edge(
                                built[reference - 2]
                                    .as_ref()
                                    .expect("transfer children are built before their parent")
                                    .as_edge(backend),
                            ),
                        };
                        let hi = branch(node.hi);
                        let lo = branch(node.lo);
                        let before = backend.num_inner_nodes();
                        let gc_before = backend.gc_count();
                        let edge = oxidd::zbdd::make_node(
                            backend,
                            self.variables[variable_map[node.variable as usize] as usize]
                                .as_edge(backend),
                            hi,
                            lo,
                        )
                        .map_err(|_| BuildError { nodes_created })?;
                        if backend.gc_count() != gc_before || backend.num_inner_nodes() > before {
                            nodes_created += 1;
                            self.record_node_created(backend.num_inner_nodes());
                        }
                        built[index] = Some(ZBDDFunction::from_edge(backend, edge));
                        state[index] = 2;
                    }
                    _ => {}
                }
            }

            let roots = dag
                .roots
                .iter()
                .map(|&reference| match reference {
                    QUERY_ZERO => Root(ZBDDFunction::empty(backend)),
                    QUERY_ONE => Root(ZBDDFunction::base(backend)),
                    _ => Root(
                        built[reference - 2]
                            .as_ref()
                            .expect("every transfer root is built")
                            .clone(),
                    ),
                })
                .collect();
            Ok((roots, nodes_created))
        })
    }
}
