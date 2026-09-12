use std::borrow::Borrow;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use oxidd::util::SatCountCache;
use oxidd::zbdd::ZBDDFunction;
use oxidd::{BooleanFunction, BooleanVecSet, Edge, Function, InnerNode, Manager, ManagerRef, Node};

pub struct ResultRow {
    pub build: Duration,
    pub filters: Duration,
    pub checksum: u128,
    pub nodes_after_build: usize,
    pub nodes_after_filters: usize,
}

fn count(function: &ZBDDFunction, variables: u32) -> u128 {
    let mut cache = SatCountCache::<u128, std::collections::hash_map::RandomState>::default();
    function.sat_count(variables, &mut cache)
}

fn count_via_public_nodes(function: &ZBDDFunction) -> u128 {
    function.with_manager_shared(|manager, root| {
        fn visit<M>(manager: &M, edge: &M::Edge, memo: &mut HashMap<usize, u128>) -> u128
        where
            M: Manager<Terminal = oxidd_rules_zbdd::ZBDDTerminal>,
        {
            let id = edge.node_id();
            if let Some(&count) = memo.get(&id) {
                return count;
            }
            let count = match manager.get_node(edge) {
                Node::Terminal(terminal) => match *terminal.borrow() {
                    oxidd_rules_zbdd::ZBDDTerminal::Empty => 0,
                    oxidd_rules_zbdd::ZBDDTerminal::Base => 1,
                },
                Node::Inner(node) => node
                    .children()
                    .map(|child| visit(manager, &child, memo))
                    .sum(),
            };
            memo.insert(id, count);
            count
        }

        visit(manager, root, &mut HashMap::new())
    })
}

fn singleton_set(
    variables: &[ZBDDFunction],
    bits: u64,
) -> Result<ZBDDFunction, oxidd::error::OutOfMemory> {
    variables[0].with_manager_shared(|manager, _| {
        let mut edge = ZBDDFunction::base_edge(manager);
        for variable in (0..variables.len()).rev() {
            if bits & (1 << variable) != 0 {
                edge = oxidd::zbdd::make_node(
                    manager,
                    variables[variable].as_edge(manager),
                    edge,
                    ZBDDFunction::empty_edge(manager),
                )?;
            }
        }
        Ok(ZBDDFunction::from_edge(manager, edge))
    })
}

fn new_variables(
    capacity: usize,
    variables: u32,
) -> (oxidd::zbdd::ZBDDManagerRef, Vec<ZBDDFunction>) {
    let manager_ref = oxidd::zbdd::new_manager(capacity, capacity / 2, 1);
    let vars = manager_ref.with_manager_exclusive(|manager| {
        manager.add_vars(variables);
        (0..variables)
            .map(|variable| ZBDDFunction::singleton(manager, variable).unwrap())
            .collect()
    });
    (manager_ref, vars)
}

fn conformance_checks() {
    let (manager_ref, variables) = new_variables(256, 4);
    let zero = manager_ref.with_manager_shared(ZBDDFunction::empty);
    let unit = manager_ref.with_manager_shared(ZBDDFunction::base);
    let powerset = manager_ref.with_manager_shared(ZBDDFunction::t);
    assert_eq!(count(&zero, 4), 0);
    assert_eq!(count(&unit, 4), 1);
    assert_eq!(count(&powerset, 4), 16);
    assert_eq!(
        count(&variables[3], 4),
        1,
        "skipped levels must not multiply count"
    );
    assert_eq!(count_via_public_nodes(&powerset), 16);

    let root = singleton_set(&variables, 0b1010).unwrap();
    let sibling = singleton_set(&variables, 0b1011).unwrap();
    let combined = root.union(&sibling).unwrap();
    assert_eq!(count(&combined, 4), 2);
    drop(manager_ref);
    assert_eq!(count(&combined, 4), 2, "a root must keep its manager alive");

    // Keep every root alive until the fixed-capacity manager reports OOM, then
    // verify that an older root is still usable.
    let (_limited_ref, limited_variables) = new_variables(48, 6);
    let stable = singleton_set(&limited_variables, 1).unwrap();
    let stable_count = count(&stable, 6);
    let mut roots = Vec::new();
    let mut failed = false;
    for bits in 0..64 {
        match singleton_set(&limited_variables, bits) {
            Ok(root) => roots.push(root),
            Err(_) => {
                failed = true;
                break;
            }
        }
    }
    assert!(
        failed,
        "the deliberately small manager should reach its node limit"
    );
    assert_eq!(count(&stable, 6), stable_count);
}

pub fn run(
    variables_count: u32,
    rounds: u32,
    sets: &[u64],
) -> Result<ResultRow, oxidd::error::OutOfMemory> {
    conformance_checks();

    let (manager_ref, variables) = new_variables(250_000, variables_count);
    let start = Instant::now();
    let mut root = manager_ref.with_manager_shared(ZBDDFunction::empty);
    for &bits in sets {
        let set = singleton_set(&variables, bits)?;
        root = root.union(&set)?;
    }
    let build = start.elapsed();
    let nodes_after_build = manager_ref.with_manager_shared(|manager| manager.num_inner_nodes());
    assert_eq!(count(&root, variables_count), sets.len() as u128);
    assert_eq!(count_via_public_nodes(&root), sets.len() as u128);

    drop(manager_ref);
    drop(variables);

    let start = Instant::now();
    let mut checksum = 0;
    for _ in 0..rounds {
        for variable in 0..variables_count {
            let included = root.subset1(variable)?.change(variable)?;
            let excluded = root.subset0(variable)?;
            checksum += count(&included, variables_count);
            checksum += count(&excluded, variables_count);
        }
    }
    let filters = start.elapsed();
    let nodes_after_filters = root.with_manager_shared(|manager, _| manager.num_inner_nodes());

    Ok(ResultRow {
        build,
        filters,
        checksum,
        nodes_after_build,
        nodes_after_filters,
    })
}
