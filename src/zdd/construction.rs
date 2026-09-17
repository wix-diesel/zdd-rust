use super::*;

impl ZddManager {
    pub(crate) fn new(
        variable_count: usize,
        node_capacity: usize,
        shared_cache_entries: usize,
    ) -> Result<Self, CreateError> {
        let variable_count_u32 =
            u32::try_from(variable_count).map_err(|_| CreateError::TooManyVariables)?;
        if node_capacity > MAX_INNER_NODES {
            return Err(CreateError::NodeCapacityTooLarge);
        }
        // The backend apply cache is deliberately disabled. Its direct-mapped
        // implementation rounds capacities up to a power of two and treats
        // zero as one entry, which cannot implement the public cache limit.
        let manager = oxidd::zbdd::new_manager(node_capacity, 0, 1);
        let (variables, powerset) = manager.with_manager_exclusive(|backend| {
            backend.add_vars(variable_count_u32);

            // Adding variables creates the backend's tautology chain. Keeping
            // every singleton root here fixes the universe and ensures later
            // node construction never needs a second variable representation.
            let variables = (0..variable_count_u32)
                .map(|variable| {
                    ZBDDFunction::singleton(backend, variable)
                        .expect("the caller reserved the complete universe capacity")
                })
                .collect();
            let powerset = ZBDDFunction::t(backend);
            (variables, powerset)
        });

        let initial_nodes = manager.with_manager_shared(|backend| backend.num_inner_nodes());
        Ok(Self {
            manager,
            variables,
            powerset,
            shared_cache: Mutex::new(SharedCache {
                capacity: shared_cache_entries,
                entries: HashMap::new(),
                insertion_order: VecDeque::new(),
                hits: 0,
                misses: 0,
                evictions: 0,
            }),
            peak_live_nodes: AtomicUsize::new(initial_nodes),
            nodes_created: AtomicUsize::new(initial_nodes),
        })
    }

    pub(crate) fn empty(&self) -> Root {
        Root(self.manager.with_manager_shared(ZBDDFunction::empty))
    }

    pub(crate) fn unit(&self) -> Root {
        Root(self.manager.with_manager_shared(ZBDDFunction::base))
    }

    pub(crate) fn powerset(&self) -> Root {
        Root(self.powerset.clone())
    }

    pub(crate) fn build_from_sets(&self, sets: &[Vec<u32>]) -> Result<(Root, usize), BuildError> {
        let mut trie = vec![TrieNode::default()];
        for set in sets {
            let mut node_index = 0;
            for &variable in set {
                let child_index = match trie[node_index]
                    .children
                    .binary_search_by_key(&variable, |&(child_variable, _)| child_variable)
                {
                    Ok(position) => trie[node_index].children[position].1,
                    Err(position) => {
                        let child_index = trie.len();
                        trie.push(TrieNode::default());
                        trie[node_index]
                            .children
                            .insert(position, (variable, child_index));
                        child_index
                    }
                };
                node_index = child_index;
            }
            trie[node_index].terminal = true;
        }

        self.manager.with_manager_shared(|backend| {
            let mut roots: Vec<Option<ZBDDFunction>> = (0..trie.len()).map(|_| None).collect();
            let mut nodes_created = 0;

            // Children are appended after their parents, so reverse index order
            // is a non-recursive post-order traversal of the trie.
            for node_index in (0..trie.len()).rev() {
                let node = &trie[node_index];
                let mut edge = if node.terminal {
                    ZBDDFunction::base_edge(backend)
                } else {
                    ZBDDFunction::empty_edge(backend)
                };

                // Lower variable indices are higher in the ZDD. Building
                // siblings in reverse order lets each new node use the
                // already-built higher-index siblings as its LO branch.
                for &(variable, child_index) in node.children.iter().rev() {
                    let nodes_before = backend.num_inner_nodes();
                    let gc_before = backend.gc_count();
                    let hi = backend.clone_edge(
                        roots[child_index]
                            .as_ref()
                            .expect("trie children are built before their parents")
                            .as_edge(backend),
                    );
                    edge = match oxidd::zbdd::make_node(
                        backend,
                        self.variables[variable as usize].as_edge(backend),
                        hi,
                        edge,
                    ) {
                        Ok(edge) => edge,
                        Err(_) => return Err(BuildError { nodes_created }),
                    };
                    if backend.gc_count() != gc_before || backend.num_inner_nodes() > nodes_before {
                        nodes_created += 1;
                        self.record_node_created(backend.num_inner_nodes());
                    }
                }
                roots[node_index] = Some(ZBDDFunction::from_edge(backend, edge));
            }

            let root = roots[0]
                .take()
                .expect("the trie always contains its root node");
            Ok((Root(root), nodes_created))
        })
    }

    pub(crate) fn roots_equal(&self, left: &Root, right: &Root) -> bool {
        left.0 == right.0
    }
}
