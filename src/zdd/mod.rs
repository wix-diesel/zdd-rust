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
    CacheCapacityTooLarge,
}

#[derive(Debug)]
pub(crate) struct OutOfMemory;

/// Owns the backend manager and all roots required by the fixed universe.
pub(crate) struct ZddManager {
    manager: ZBDDManagerRef,
    variables: Vec<ZBDDFunction>,
    powerset: ZBDDFunction,
}

#[derive(Clone)]
pub(crate) struct Root(ZBDDFunction);

impl ZddManager {
    pub(crate) fn new(
        variable_count: usize,
        node_capacity: usize,
        cache_capacity: usize,
    ) -> Result<Self, CreateError> {
        let variable_count_u32 =
            u32::try_from(variable_count).map_err(|_| CreateError::TooManyVariables)?;
        if node_capacity > MAX_INNER_NODES {
            return Err(CreateError::NodeCapacityTooLarge);
        }
        if cache_capacity.checked_next_power_of_two().is_none() {
            return Err(CreateError::CacheCapacityTooLarge);
        }

        let manager = oxidd::zbdd::new_manager(node_capacity, cache_capacity, 1);
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

    pub(crate) fn build_from_sets(&self, sets: &[Vec<u32>]) -> Result<Root, OutOfMemory> {
        let mut family = self.empty().0;
        for set in sets {
            let singleton = self.manager.with_manager_shared(|backend| {
                let mut edge = ZBDDFunction::base_edge(backend);
                for &variable in set.iter().rev() {
                    edge = oxidd::zbdd::make_node(
                        backend,
                        self.variables[variable as usize].as_edge(backend),
                        edge,
                        ZBDDFunction::empty_edge(backend),
                    )?;
                }
                Ok::<_, oxidd::error::OutOfMemory>(ZBDDFunction::from_edge(backend, edge))
            });
            let singleton = singleton.map_err(|_| OutOfMemory)?;
            family = family.union(&singleton).map_err(|_| OutOfMemory)?;
        }
        Ok(Root(family))
    }

    pub(crate) fn roots_equal(&self, left: &Root, right: &Root) -> bool {
        left.0 == right.0
    }

    #[cfg(test)]
    pub(crate) fn inner_node_count(&self) -> usize {
        self.manager
            .with_manager_shared(|backend| backend.num_inner_nodes())
    }

    #[cfg(test)]
    fn count(&self, root: &Root) -> u128 {
        let mut cache = SatCountCache::<u128, std::collections::hash_map::RandomState>::default();
        root.0.sat_count(self.variables.len() as u32, &mut cache)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals_and_powerset_have_zdd_semantics() {
        let manager = ZddManager::new(3, 64, 8).unwrap();
        assert_eq!(manager.count(&manager.empty()), 0);
        assert_eq!(manager.count(&manager.unit()), 1);
        assert_eq!(manager.count(&manager.powerset()), 8);
        assert!(!manager.roots_equal(&manager.empty(), &manager.unit()));
    }

    #[test]
    fn empty_universe_powerset_is_unit() {
        let manager = ZddManager::new(0, 0, 0).unwrap();
        assert!(manager.roots_equal(&manager.powerset(), &manager.unit()));
    }

    #[test]
    fn unique_table_reuses_nodes_and_lo_equals_hi_is_preserved() {
        let manager = ZddManager::new(1, 16, 8).unwrap();
        let before = manager.inner_node_count();
        let first = manager.build_from_sets(&[vec![], vec![0]]).unwrap();
        let after_first = manager.inner_node_count();
        let second = manager.build_from_sets(&[vec![0], vec![]]).unwrap();

        assert_eq!(manager.count(&first), 2);
        assert!(manager.roots_equal(&first, &second));
        assert!(after_first >= before);
        assert_eq!(manager.inner_node_count(), after_first);
        assert!(!manager.roots_equal(&first, &manager.unit()));
    }

    #[test]
    fn table_growth_keeps_canonical_roots() {
        let manager = ZddManager::new(10, 4_096, 32).unwrap();
        let sets: Vec<Vec<u32>> = (0u32..512)
            .map(|bits| (0..10).filter(|v| bits & (1 << v) != 0).collect())
            .collect();
        let first = manager.build_from_sets(&sets).unwrap();
        let second = manager.build_from_sets(&sets).unwrap();
        assert!(manager.roots_equal(&first, &second));
        assert_eq!(manager.count(&first), 512);
    }
}
