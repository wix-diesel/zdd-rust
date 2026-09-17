use super::*;
use crate::test_support::{OracleFamily, oracle_family_strategy};
use proptest::prelude::*;

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

fn property_test_config() -> ProptestConfig {
    let cases = std::env::var("ZDD_PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(64);
    ProptestConfig {
        cases,
        ..ProptestConfig::default()
    }
}

#[test]
fn default_limits_match_the_public_contract() {
    let limits = Limits::default();
    assert_eq!(limits.max_live_nodes, 1_000_000);
    assert_eq!(limits.max_operation_memo_entries, 1_000_000);
    assert_eq!(limits.max_frontier_states, 1_000_000);
    assert_eq!(limits.max_frontier_transitions, 10_000_000);
    assert_eq!(limits.shared_cache_entries, 262_144);

    let query_limits = QueryLimits::default();
    assert_eq!(query_limits.max_snapshot_nodes, 1_000_000);
    assert_eq!(query_limits.max_total_count_bits, 67_108_864);
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
fn exact_counts_cover_terminals_powersets_and_skipped_variables() {
    let empty_space = small_space(0);
    assert_eq!(empty_space.empty().count(), BigUint::from(0u8));
    assert_eq!(empty_space.unit().count(), BigUint::from(1u8));
    assert_eq!(empty_space.powerset().unwrap().count(), BigUint::from(1u8));

    let space = small_space(5);
    assert_eq!(space.powerset().unwrap().count(), BigUint::from(32u8));
    let last = space.variable(4).unwrap();
    let skipped = space.from_sets([vec![], vec![last]]).unwrap();
    assert_eq!(skipped.count(), BigUint::from(2u8));
    assert_eq!(skipped.try_count_u128(), Ok(2));
}

#[test]
fn fixed_width_count_reports_overflow_but_exact_count_does_not() {
    let boundary_space = small_space(128);
    let boundary = boundary_space
        .powerset()
        .unwrap()
        .difference(&boundary_space.unit())
        .unwrap();
    assert_eq!(boundary.try_count_u128(), Ok(u128::MAX));

    let space = small_space(129);
    let family = space.powerset().unwrap();
    assert_eq!(family.count(), BigUint::from(1u8) << 129usize);
    assert_eq!(family.try_count_u128(), Err(CountError::Overflow));
}

#[test]
fn iterator_distinguishes_terminals_and_uses_exclude_first_order() {
    let space = small_space(3);
    let a = space.variable(0).unwrap();
    let b = space.variable(1).unwrap();
    let c = space.variable(2).unwrap();

    assert_eq!(space.empty().iter().next(), None);
    assert_eq!(
        space
            .unit()
            .iter()
            .map(Solution::into_vec)
            .collect::<Vec<_>>(),
        vec![vec![]]
    );
    assert_eq!(
        space
            .powerset()
            .unwrap()
            .iter()
            .map(Solution::into_vec)
            .collect::<Vec<_>>(),
        vec![
            vec![],
            vec![c],
            vec![b],
            vec![b, c],
            vec![a],
            vec![a, c],
            vec![a, b],
            vec![a, b, c],
        ]
    );
}

#[test]
fn every_small_family_is_enumerated_once_and_deterministically() {
    let space = small_space(3);
    let exclude_first = [0u64, 4, 2, 6, 1, 5, 3, 7];

    for family_mask in 0u64..=255 {
        let family = OracleFamily::from_family_mask(3, family_mask).build_in(&space);
        let actual = family
            .iter()
            .map(|solution| {
                solution
                    .iter()
                    .fold(0u64, |set, variable| set | (1 << variable.index()))
            })
            .collect::<Vec<_>>();
        let expected = exclude_first
            .iter()
            .copied()
            .filter(|set| family_mask & (1 << set) != 0)
            .collect::<Vec<_>>();

        assert_eq!(actual, expected, "family mask {family_mask:#010b}");
        assert_eq!(
            family.iter().collect::<Vec<_>>(),
            family.iter().collect::<Vec<_>>()
        );
    }
}

#[test]
fn iterator_is_detached_and_owned_solutions_outlive_it() {
    let (mut iterator, a) = {
        let space = small_space(1);
        let a = space.variable(0).unwrap();
        (space.powerset().unwrap().iter(), a)
    };

    let empty = iterator.next().unwrap();
    let selected = iterator.next().unwrap();
    drop(iterator);
    assert!(empty.is_empty());
    assert_eq!(selected.as_slice(), &[a]);
}

#[test]
fn large_family_take_visits_only_a_prefix_without_counting() {
    let space = small_space(129);
    let last = space.variable(128).unwrap();
    let penultimate = space.variable(127).unwrap();
    let mut iterator = space.powerset().unwrap().iter();

    assert_eq!(iterator.size_hint(), (0, None));
    assert_eq!(
        iterator
            .by_ref()
            .take(3)
            .map(Solution::into_vec)
            .collect::<Vec<_>>(),
        vec![vec![], vec![last], vec![penultimate]]
    );
    assert_eq!(iterator.size_hint(), (0, None));
}

#[test]
fn visitor_reuses_traversal_state_allows_reentry_and_stops_early() {
    let space = small_space(6);
    let family = space.powerset().unwrap();
    let mut visited = Vec::new();

    let result = family.visit_solutions(|solution| {
        assert!(family.contains(solution).unwrap());
        assert_eq!(
            family.intersection(&space.unit()).unwrap().count(),
            1u8.into()
        );
        visited.push(solution.to_vec());
        if visited.len() == 5 {
            ControlFlow::Break("enough")
        } else {
            ControlFlow::Continue(())
        }
    });

    assert_eq!(result, ControlFlow::Break("enough"));
    assert_eq!(visited.len(), 5);
    assert_eq!(
        visited,
        family
            .iter()
            .take(5)
            .map(Solution::into_vec)
            .collect::<Vec<_>>()
    );
}

#[test]
fn visitor_handles_empty_family_and_empty_solution() {
    let space = small_space(0);
    let mut empty_calls = 0;
    assert_eq!(
        space.empty().visit_solutions::<()>(|_| {
            empty_calls += 1;
            ControlFlow::Continue(())
        }),
        ControlFlow::Continue(())
    );
    assert_eq!(empty_calls, 0);

    let mut unit_calls = 0;
    assert_eq!(
        space.unit().visit_solutions::<()>(|solution| {
            unit_calls += 1;
            assert!(solution.is_empty());
            ControlFlow::Continue(())
        }),
        ControlFlow::Continue(())
    );
    assert_eq!(unit_calls, 1);
}

#[test]
fn count_index_is_detached_and_reports_logical_memory() {
    let index = {
        let space = small_space(3);
        let family = space.powerset().unwrap();
        family.count_index(&QueryLimits::default()).unwrap()
    };

    assert_eq!(index.count(), &BigUint::from(8u8));
    assert_eq!(index.stats().snapshot_nodes, 3);
    assert_eq!(index.stats().total_count_bits, 10);
    assert_eq!(index.stats().max_count_bits, 4);
}

#[test]
fn count_index_enforces_node_and_count_bit_boundaries() {
    let space = small_space(3);
    let family = space.powerset().unwrap();

    let exact = QueryLimits {
        max_snapshot_nodes: 3,
        max_total_count_bits: 10,
    };
    assert_eq!(
        family.count_index(&exact).unwrap().count(),
        &BigUint::from(8u8)
    );

    let node_error = family
        .count_index(&QueryLimits {
            max_snapshot_nodes: 2,
            ..exact.clone()
        })
        .unwrap_err();
    assert!(matches!(
        node_error,
        QueryError::LimitExceeded {
            kind: LimitKind::QueryNodes,
            limit: 2,
            attempted: 3,
            stats: QueryStats {
                snapshot_nodes: 2,
                ..
            },
        }
    ));

    let bit_error = family
        .count_index(&QueryLimits {
            max_total_count_bits: 9,
            ..exact
        })
        .unwrap_err();
    assert!(matches!(
        bit_error,
        QueryError::LimitExceeded {
            kind: LimitKind::CountBits,
            limit: 9,
            attempted: 10,
            stats: QueryStats {
                total_count_bits: 6,
                max_count_bits: 3,
                ..
            },
        }
    ));
}

#[test]
fn zero_count_needs_no_count_bits_but_unit_needs_one() {
    let space = small_space(0);
    let limits = QueryLimits {
        max_snapshot_nodes: 0,
        max_total_count_bits: 0,
    };
    assert_eq!(
        space.empty().count_index(&limits).unwrap().count(),
        &BigUint::from(0u8)
    );
    assert!(matches!(
        space.unit().count_index(&limits),
        Err(QueryError::LimitExceeded {
            kind: LimitKind::CountBits,
            attempted: 1,
            ..
        })
    ));
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
    assert_eq!(family.count(), BigUint::from(2u8));
    assert!(!family.is_empty());
    let result = family.intersection(&space.powerset().unwrap()).unwrap();
    assert!(result.equivalent(&family).unwrap());
}

#[test]
fn all_three_variable_families_match_the_explicit_oracle() {
    let space = FamilySpace::builder(3)
        .limits(Limits {
            max_live_nodes: 8_192,
            max_operation_memo_entries: 1_024,
            shared_cache_entries: 17,
            ..Limits::default()
        })
        .build()
        .unwrap();
    let oracles = (0u64..=255)
        .map(|mask| OracleFamily::from_family_mask(3, mask))
        .collect::<Vec<_>>();
    let families = oracles
        .iter()
        .map(|oracle| oracle.build_in(&space))
        .collect::<Vec<_>>();

    for left in 0usize..=255 {
        for right in 0usize..=255 {
            let union = families[left].union(&families[right]).unwrap();
            let intersection = families[left].intersection(&families[right]).unwrap();
            let difference = families[left].difference(&families[right]).unwrap();
            let symmetric_difference = families[left]
                .symmetric_difference(&families[right])
                .unwrap();

            oracles[left]
                .union(&oracles[right])
                .assert_matches(&space, &union);
            oracles[left]
                .intersection(&oracles[right])
                .assert_matches(&space, &intersection);
            oracles[left]
                .difference(&oracles[right])
                .assert_matches(&space, &difference);
            oracles[left]
                .symmetric_difference(&oracles[right])
                .assert_matches(&space, &symmetric_difference);
            assert_eq!(
                families[left].is_subset_of(&families[right]).unwrap(),
                oracles[left].is_subset_of(&oracles[right])
            );
        }
    }

    for (oracle, family) in oracles.iter().zip(&families) {
        oracle.assert_matches(&space, family);
    }
}

#[test]
fn all_four_variable_families_pass_unary_and_normalization_checks() {
    let space = FamilySpace::builder(4)
        .limits(Limits {
            max_live_nodes: 16_384,
            shared_cache_entries: 0,
            ..Limits::default()
        })
        .build()
        .unwrap();

    for mask in 0u64..=u16::MAX as u64 {
        let oracle = OracleFamily::from_family_mask(4, mask);
        let family = oracle.build_in_with_normalization_noise(&space);
        oracle.assert_matches(&space, &family);
        assert_eq!(family.count(), BigUint::from(oracle.count()));
        assert!(family.equivalent(&oracle.build_in(&space)).unwrap());
    }
}

proptest! {
    #![proptest_config(property_test_config())]

    #[test]
    fn family_operations_obey_algebra_and_preserve_inputs(
        left_oracle in oracle_family_strategy(4),
        middle_oracle in oracle_family_strategy(4),
        right_oracle in oracle_family_strategy(4),
    ) {
        let space = small_space(4);
        let left = left_oracle.build_in_with_normalization_noise(&space);
        let middle = middle_oracle.build_in(&space);
        let right = right_oracle.build_in_with_normalization_noise(&space);

        let union_lr = left.union(&right).unwrap();
        let union_rl = right.union(&left).unwrap();
        prop_assert!(union_lr.equivalent(&union_rl).unwrap());
        prop_assert!(left.union(&left).unwrap().equivalent(&left).unwrap());
        prop_assert!(left.union(&middle).unwrap().union(&right).unwrap()
            .equivalent(&left.union(&middle.union(&right).unwrap()).unwrap()).unwrap());

        let intersection_lr = left.intersection(&right).unwrap();
        let intersection_rl = right.intersection(&left).unwrap();
        prop_assert!(intersection_lr.equivalent(&intersection_rl).unwrap());
        prop_assert!(left.intersection(&left).unwrap().equivalent(&left).unwrap());
        prop_assert!(left.intersection(&middle).unwrap().intersection(&right).unwrap()
            .equivalent(&left.intersection(&middle.intersection(&right).unwrap()).unwrap()).unwrap());

        prop_assert!(left.difference(&left).unwrap().is_empty());
        prop_assert!(left.symmetric_difference(&left).unwrap().is_empty());

        left_oracle.union(&right_oracle).assert_matches(&space, &union_lr);
        left_oracle.intersection(&right_oracle).assert_matches(&space, &intersection_lr);
        left_oracle.difference(&right_oracle)
            .assert_matches(&space, &left.difference(&right).unwrap());
        left_oracle.symmetric_difference(&right_oracle)
            .assert_matches(&space, &left.symmetric_difference(&right).unwrap());

        // Every operation above must leave its immutable inputs unchanged.
        left_oracle.assert_matches(&space, &left);
        middle_oracle.assert_matches(&space, &middle);
        right_oracle.assert_matches(&space, &right);
    }
}

#[test]
fn cross_space_binary_operations_and_comparisons_are_rejected() {
    let left = small_space(1).unit();
    let right = small_space(1).unit();

    assert!(matches!(
        left.union(&right),
        Err(Error::ContextMismatch { .. })
    ));
    assert!(matches!(
        left.intersection(&right),
        Err(Error::ContextMismatch { .. })
    ));
    assert!(matches!(
        left.difference(&right),
        Err(Error::ContextMismatch { .. })
    ));
    assert!(matches!(
        left.symmetric_difference(&right),
        Err(Error::ContextMismatch { .. })
    ));
    assert!(matches!(
        left.equivalent(&right),
        Err(Error::ContextMismatch { .. })
    ));
    assert!(matches!(
        left.is_subset_of(&right),
        Err(Error::ContextMismatch { .. })
    ));
}

#[test]
fn contains_normalizes_order_and_duplicates() {
    let space = small_space(3);
    let a = space.variable(0).unwrap();
    let c = space.variable(2).unwrap();
    let family = space.from_sets([vec![a, c]]).unwrap();

    assert!(family.contains(&[c, a, c]).unwrap());
    assert!(!family.contains(&[a]).unwrap());
}

#[test]
fn all_filters_match_the_explicit_oracle_and_partition_families() {
    let space = small_space(3);
    let variables = (0..3)
        .map(|index| space.variable(index).unwrap())
        .collect::<Vec<_>>();

    for mask in 0u64..=255 {
        let oracle = OracleFamily::from_family_mask(3, mask);
        let family = oracle.build_in(&space);

        for (variable, &element) in variables.iter().enumerate() {
            let containing = family.filter_contains(element).unwrap();
            let excluding = family.filter_excludes(element).unwrap();
            oracle
                .filter_contains(variable)
                .assert_matches(&space, &containing);
            oracle
                .filter_excludes(variable)
                .assert_matches(&space, &excluding);
            assert!(containing.intersection(&excluding).unwrap().is_empty());
            assert!(
                containing
                    .union(&excluding)
                    .unwrap()
                    .equivalent(&family)
                    .unwrap()
            );
        }

        for target in 0u64..8 {
            let elements = (0..3)
                .rev()
                .filter(|variable| target & (1 << variable) != 0)
                .flat_map(|variable| [variables[variable], variables[variable]])
                .collect::<Vec<_>>();
            oracle
                .filter_subsets_of(target)
                .assert_matches(&space, &family.filter_subsets_of(&elements).unwrap());
            oracle
                .filter_supersets_of(target)
                .assert_matches(&space, &family.filter_supersets_of(&elements).unwrap());
        }

        for lower in 0..=4 {
            for upper in lower..=4 {
                oracle.filter_cardinality(lower, upper).assert_matches(
                    &space,
                    &family.cardinality().between(lower..=upper).unwrap(),
                );
            }
        }
    }
}

#[test]
fn filter_boundaries_and_invalid_inputs_are_explicit() {
    let space = small_space(2);
    let a = space.variable(0).unwrap();
    let family = space.powerset().unwrap();

    assert!(family.cardinality().exactly(3).unwrap().is_empty());
    assert!(
        family
            .cardinality()
            .at_least(0)
            .unwrap()
            .equivalent(&family)
            .unwrap()
    );
    assert!(
        family
            .cardinality()
            .at_most(usize::MAX)
            .unwrap()
            .equivalent(&family)
            .unwrap()
    );
    assert!(
        family
            .cardinality()
            .at_least(usize::MAX)
            .unwrap()
            .is_empty()
    );
    let reversed_start = 2;
    let reversed_end = 1;
    assert!(matches!(
        family.cardinality().between(reversed_start..=reversed_end),
        Err(Error::InvalidRange { start: 2, end: 1 })
    ));
    assert!(
        family
            .filter_supersets_of(&[])
            .unwrap()
            .equivalent(&family)
            .unwrap()
    );
    let only_empty = family.filter_subsets_of(&[]).unwrap();
    assert!(only_empty.contains(&[]).unwrap());
    assert!(!only_empty.contains(&[a]).unwrap());

    let larger_space = small_space(3);
    let out_of_range = larger_space.variable(2).unwrap();
    assert!(matches!(
        family.filter_contains(out_of_range),
        Err(Error::InvalidElement { index: 2, .. })
    ));
    assert!(matches!(
        family.filter_subsets_of(&[out_of_range]),
        Err(Error::InvalidElement { index: 2, .. })
    ));
}

#[test]
fn filtering_composes_with_construction_intersection_and_queries() {
    let space = small_space(4);
    let variables = (0..4)
        .map(|index| space.variable(index).unwrap())
        .collect::<Vec<_>>();
    let left = space.powerset().unwrap();
    let right = space
        .from_sets([
            vec![variables[0], variables[1]],
            vec![variables[0], variables[2], variables[3]],
            vec![variables[1], variables[2]],
        ])
        .unwrap();

    let result = left
        .intersection(&right)
        .unwrap()
        .filter_contains(variables[0])
        .unwrap()
        .cardinality()
        .exactly(2)
        .unwrap();
    assert!(result.contains(&[variables[0], variables[1]]).unwrap());
    assert!(
        !result
            .contains(&[variables[0], variables[2], variables[3]])
            .unwrap()
    );
    assert!(!result.contains(&[variables[1], variables[2]]).unwrap());
}

#[test]
fn filters_honor_operation_memo_limits_without_invalidating_inputs() {
    let space = FamilySpace::builder(2)
        .limits(Limits {
            max_live_nodes: 64,
            max_operation_memo_entries: 0,
            shared_cache_entries: 0,
            ..Limits::default()
        })
        .build()
        .unwrap();
    let family = space.powerset().unwrap();
    let a = space.variable(0).unwrap();

    assert!(matches!(
        family.filter_contains(a),
        Err(Error::LimitExceeded {
            kind: LimitKind::OperationMemo,
            limit: 0,
            attempted: 1,
            ..
        })
    ));
    assert!(family.contains(&[]).unwrap());
    assert!(family.contains(&[a]).unwrap());
}

#[test]
fn filtering_a_deep_zdd_does_not_use_the_call_stack() {
    const VARIABLE_COUNT: usize = 2_000;
    let space = FamilySpace::builder(VARIABLE_COUNT)
        .limits(Limits {
            max_live_nodes: 40_000,
            max_operation_memo_entries: 20_000,
            shared_cache_entries: 0,
            ..Limits::default()
        })
        .build()
        .unwrap();
    let variables = (0..VARIABLE_COUNT)
        .map(|index| space.variable(index).unwrap())
        .collect::<Vec<_>>();
    let family = space.from_sets([variables.clone()]).unwrap();

    let result = family
        .filter_contains(variables[VARIABLE_COUNT - 1])
        .unwrap()
        .cardinality()
        .exactly(VARIABLE_COUNT)
        .unwrap();
    assert!(result.contains(&variables).unwrap());
}

#[test]
fn operation_memo_limit_is_an_explicit_error() {
    let space = FamilySpace::builder(2)
        .limits(Limits {
            max_live_nodes: 64,
            max_operation_memo_entries: 0,
            shared_cache_entries: 0,
            ..Limits::default()
        })
        .build()
        .unwrap();
    let a = space.from_sets([vec![space.variable(0).unwrap()]]).unwrap();
    let b = space.from_sets([vec![space.variable(1).unwrap()]]).unwrap();

    assert!(matches!(
        a.union(&b),
        Err(Error::LimitExceeded {
            kind: LimitKind::OperationMemo,
            limit: 0,
            attempted: 1,
            ..
        })
    ));
    assert!(a.contains(&[space.variable(0).unwrap()]).unwrap());
    assert!(b.contains(&[space.variable(1).unwrap()]).unwrap());
}

#[test]
fn cache_eviction_and_cache_disable_do_not_change_results() {
    for cache_capacity in [0, 1] {
        let space = FamilySpace::builder(3)
            .limits(Limits {
                max_live_nodes: 256,
                shared_cache_entries: cache_capacity,
                ..Limits::default()
            })
            .build()
            .unwrap();
        let left_oracle = OracleFamily::from_family_mask(3, 0b1010_1010);
        let right_oracle = OracleFamily::from_family_mask(3, 0b1100_1100);
        let left = left_oracle.build_in(&space);
        let right = right_oracle.build_in(&space);

        left_oracle
            .union(&right_oracle)
            .assert_matches(&space, &left.union(&right).unwrap());
        left_oracle
            .intersection(&right_oracle)
            .assert_matches(&space, &left.intersection(&right).unwrap());
        left_oracle
            .difference(&right_oracle)
            .assert_matches(&space, &left.difference(&right).unwrap());
        left_oracle
            .symmetric_difference(&right_oracle)
            .assert_matches(&space, &left.symmetric_difference(&right).unwrap());

        let stats = space.stats();
        assert!(stats.shared_cache_entries <= cache_capacity);
        if cache_capacity == 0 {
            assert_eq!(stats.shared_cache_evictions, 0);
        } else {
            assert!(stats.shared_cache_evictions > 0);
        }
    }
}
