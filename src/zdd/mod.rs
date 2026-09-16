//! Internal adapter around the selected ZDD backend.

#[cfg(test)]
use oxidd::util::SatCountCache;
use oxidd::zbdd::{ZBDDFunction, ZBDDManagerRef};
use oxidd::{BooleanFunction, BooleanVecSet, Function, Manager, ManagerRef};

const TERMINAL_NODES: usize = 2;
const MAX_INNER_NODES: usize = (u32::MAX as usize) - (TERMINAL_NODES - 1);

#[derive(Debug)]
pub(crate) enum CreateError {
    TooManyVariables,
    NodeCapacityTooLarge,
}

#[derive(Debug)]
pub(crate) struct BuildError {
    pub(crate) nodes_created: usize,
}

#[derive(Default)]
struct TrieNode {
    terminal: bool,
    children: Vec<(u32, usize)>,
}

/// Owns the backend manager and all roots required by the fixed universe.
pub(crate) struct ZddManager {
    manager: ZBDDManagerRef,
    variables: Vec<ZBDDFunction>,
    powerset: ZBDDFunction,
}

#[derive(Clone)]
pub(crate) struct Root(ZBDDFunction);

impl ZddManager {
    pub(crate) fn new(variable_count: usize, node_capacity: usize) -> Result<Self, CreateError> {
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

        Ok(Self {
            manager,
            variables,
            powerset,
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

    pub(crate) fn inner_node_count(&self) -> usize {
        self.manager
            .with_manager_shared(|backend| backend.num_inner_nodes())
    }

    #[cfg(test)]
    fn count(&self, root: &Root) -> u128 {
        let mut cache = SatCountCache::<u128, std::collections::hash_map::RandomState>::default();
        root.0.sat_count(self.variables.len() as u32, &mut cache)
    }

    #[cfg(test)]
    fn contains(&self, root: &Root, set: &[u32]) -> bool {
        let mut remainder = root.0.clone();
        for variable in 0..self.variables.len() as u32 {
            remainder = if set.binary_search(&variable).is_ok() {
                remainder.subset1(variable).unwrap()
            } else {
                remainder.subset0(variable).unwrap()
            };
        }
        remainder == self.unit().0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals_and_powerset_have_zdd_semantics() {
        let manager = ZddManager::new(3, 64).unwrap();
        assert_eq!(manager.count(&manager.empty()), 0);
        assert_eq!(manager.count(&manager.unit()), 1);
        assert_eq!(manager.count(&manager.powerset()), 8);
        assert!(!manager.roots_equal(&manager.empty(), &manager.unit()));
    }

    #[test]
    fn empty_universe_powerset_is_unit() {
        let manager = ZddManager::new(0, 0).unwrap();
        assert!(manager.roots_equal(&manager.powerset(), &manager.unit()));
    }

    #[test]
    fn unique_table_reuses_nodes_and_lo_equals_hi_is_preserved() {
        let manager = ZddManager::new(1, 16).unwrap();
        let before = manager.inner_node_count();
        let first = manager.build_from_sets(&[vec![], vec![0]]).unwrap().0;
        let after_first = manager.inner_node_count();
        let second = manager.build_from_sets(&[vec![0], vec![]]).unwrap().0;

        assert_eq!(manager.count(&first), 2);
        assert!(manager.roots_equal(&first, &second));
        assert!(after_first >= before);
        assert_eq!(manager.inner_node_count(), after_first);
        assert!(!manager.roots_equal(&first, &manager.unit()));
    }

    #[test]
    fn table_growth_keeps_canonical_roots() {
        let manager = ZddManager::new(10, 4_096).unwrap();
        let sets: Vec<Vec<u32>> = (0u32..512)
            .map(|bits| (0..10).filter(|v| bits & (1 << v) != 0).collect())
            .collect();
        let first = manager.build_from_sets(&sets).unwrap().0;
        let second = manager.build_from_sets(&sets).unwrap().0;
        assert!(manager.roots_equal(&first, &second));
        assert_eq!(manager.count(&first), 512);
    }

    #[test]
    fn explicit_sets_match_an_independent_membership_oracle() {
        let manager = ZddManager::new(3, 64).unwrap();
        let expected = [vec![], vec![0], vec![0, 2], vec![1, 2]];
        let root = manager.build_from_sets(&expected).unwrap().0;

        for bits in 0u32..8 {
            let candidate: Vec<u32> = (0..3)
                .filter(|variable| bits & (1 << variable) != 0)
                .collect();
            assert_eq!(
                manager.contains(&root, &candidate),
                expected.contains(&candidate),
                "membership differed for {candidate:?}"
            );
        }
    }
}
