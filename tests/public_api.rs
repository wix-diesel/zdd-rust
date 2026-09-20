use std::collections::BTreeSet;
use std::ops::ControlFlow;

use zdd_family::{
    BigUint, CountError, Error, FamilySpace, LimitKind, Limits, QueryError, QueryLimits, SetFamily,
    VariableId,
};

fn variables(space: &FamilySpace, count: usize) -> Vec<VariableId> {
    (0..count)
        .map(|index| space.variable(index).unwrap())
        .collect()
}

fn family_from_mask(space: &FamilySpace, variables: &[VariableId], family_mask: u64) -> SetFamily {
    let set_count = 1usize << variables.len();
    let sets = (0..set_count)
        .filter(|set_mask| family_mask & (1u64 << set_mask) != 0)
        .map(|set_mask| {
            variables
                .iter()
                .enumerate()
                .filter(|(index, _)| set_mask & (1 << index) != 0)
                .map(|(_, variable)| *variable)
                .collect::<Vec<_>>()
        });
    space.from_sets(sets).unwrap()
}

fn set_masks(family: &SetFamily) -> BTreeSet<u64> {
    family
        .iter()
        .map(|solution| {
            solution
                .iter()
                .fold(0u64, |mask, variable| mask | (1 << variable.index()))
        })
        .collect()
}

#[test]
fn terminals_powerset_and_explicit_sets_have_distinct_semantics() {
    let space = FamilySpace::new(3).unwrap();
    let vars = variables(&space, 3);

    assert!(space.empty().is_empty());
    assert_eq!(set_masks(&space.unit()), BTreeSet::from([0]));
    assert_eq!(
        set_masks(&space.powerset().unwrap()),
        (0..8).collect::<BTreeSet<_>>()
    );

    let family = space
        .from_sets([
            vec![vars[2], vars[0], vars[0]],
            vec![],
            vec![vars[0], vars[2]],
        ])
        .unwrap();
    assert_eq!(set_masks(&family), BTreeSet::from([0, 0b101]));
    assert!(family.contains(&[vars[2], vars[0], vars[2]]).unwrap());
}

#[test]
fn binary_operations_match_an_explicit_oracle() {
    let space = FamilySpace::new(3).unwrap();
    let vars = variables(&space, 3);

    for seed in 0u64..64 {
        let left_mask = seed.wrapping_mul(0x9d) as u8;
        let right_mask = seed.wrapping_mul(0x67).wrapping_add(0x35) as u8;
        let left = family_from_mask(&space, &vars, u64::from(left_mask));
        let right = family_from_mask(&space, &vars, u64::from(right_mask));

        let expected = |mask: u8| {
            (0..8)
                .filter(|set| mask & (1 << set) != 0)
                .map(|set| set as u64)
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(
            set_masks(&left.union(&right).unwrap()),
            expected(left_mask | right_mask)
        );
        assert_eq!(
            set_masks(&left.intersection(&right).unwrap()),
            expected(left_mask & right_mask)
        );
        assert_eq!(
            set_masks(&left.difference(&right).unwrap()),
            expected(left_mask & !right_mask)
        );
        assert_eq!(
            set_masks(&left.symmetric_difference(&right).unwrap()),
            expected(left_mask ^ right_mask)
        );
    }
}

#[test]
fn filters_match_naive_set_predicates() {
    let space = FamilySpace::new(4).unwrap();
    let vars = variables(&space, 4);
    let all = space.powerset().unwrap();

    assert_eq!(
        set_masks(&all.filter_contains(vars[1]).unwrap()),
        (0u64..16).filter(|set| set & 0b0010 != 0).collect()
    );
    assert_eq!(
        set_masks(&all.filter_excludes(vars[1]).unwrap()),
        (0u64..16).filter(|set| set & 0b0010 == 0).collect()
    );
    assert_eq!(
        set_masks(&all.filter_subsets_of(&[vars[0], vars[2]]).unwrap()),
        (0u64..16).filter(|set| set & !0b0101 == 0).collect()
    );
    assert_eq!(
        set_masks(&all.filter_supersets_of(&[vars[0], vars[2]]).unwrap()),
        (0u64..16).filter(|set| set & 0b0101 == 0b0101).collect()
    );
    assert_eq!(
        set_masks(&all.cardinality().between(1..=2).unwrap()),
        (0u64..16)
            .filter(|set| set.count_ones() == 1 || set.count_ones() == 2)
            .collect()
    );
}

#[test]
fn counting_iteration_and_visitor_agree() {
    let space = FamilySpace::new(5).unwrap();
    let vars = variables(&space, 5);
    let family = family_from_mask(&space, &vars, 0x9ace_f135);

    let iterated = family
        .iter()
        .map(|solution| solution.into_vec())
        .collect::<Vec<_>>();
    let mut visited = Vec::new();
    assert_eq!(
        family.visit_solutions::<()>(|solution| {
            visited.push(solution.to_vec());
            ControlFlow::Continue(())
        }),
        ControlFlow::Continue(())
    );

    assert_eq!(iterated, visited);
    assert_eq!(family.count(), BigUint::from(iterated.len()));
    assert_eq!(family.try_count_u128(), Ok(iterated.len() as u128));
    let index = family.count_index(&QueryLimits::default()).unwrap();
    assert_eq!(index.count(), &family.count());
    assert!(index.stats().snapshot_nodes > 0);
}

#[test]
fn public_errors_preserve_their_categories() {
    let space = FamilySpace::new(2).unwrap();
    assert!(matches!(
        space.variable(2),
        Err(Error::InvalidElement { .. })
    ));

    let other = FamilySpace::new(3).unwrap();
    let foreign = other.variable(2).unwrap();
    assert!(matches!(
        space.from_sets([vec![foreign]]),
        Err(Error::InvalidElement { .. })
    ));
    assert!(matches!(
        space.unit().union(&other.unit()),
        Err(Error::ContextMismatch { .. })
    ));
    let (start, end) = (2, 1);
    assert!(matches!(
        space.unit().cardinality().between(start..=end),
        Err(Error::InvalidRange { .. })
    ));

    let too_small = FamilySpace::builder(2)
        .limits(Limits {
            max_live_nodes: 3,
            ..Limits::default()
        })
        .build();
    assert!(matches!(
        too_small,
        Err(Error::LimitExceeded {
            kind: LimitKind::Node,
            ..
        })
    ));
}

#[test]
fn query_limits_and_fixed_width_overflow_are_reported() {
    let space = FamilySpace::new(129).unwrap();
    let powerset = space.powerset().unwrap();
    assert_eq!(powerset.try_count_u128(), Err(CountError::Overflow));

    let error = powerset
        .count_index(&QueryLimits {
            max_snapshot_nodes: 0,
            max_total_count_bits: usize::MAX,
        })
        .unwrap_err();
    assert!(matches!(
        error,
        QueryError::LimitExceeded {
            kind: LimitKind::QueryNodes,
            ..
        }
    ));
}

#[test]
fn explicit_import_preserves_values_counts_and_enumeration_order() {
    let source = FamilySpace::new(3).unwrap();
    let source_variables = variables(&source, 3);
    let family = source
        .from_sets([
            vec![],
            vec![source_variables[0]],
            vec![source_variables[0], source_variables[2]],
            vec![source_variables[1]],
        ])
        .unwrap();
    let destination = FamilySpace::new(3).unwrap();
    let variable_map = variables(&destination, 3);

    let imported = destination.import(&family, &variable_map).unwrap();

    assert_eq!(imported.count(), family.count());
    assert_eq!(set_masks(&imported), set_masks(&family));
    assert_eq!(
        imported.iter().collect::<Vec<_>>(),
        family.iter().collect::<Vec<_>>()
    );
    assert!(matches!(
        imported.union(&family),
        Err(Error::ContextMismatch { .. })
    ));

    for family_mask in 0u64..256 {
        let source_family = family_from_mask(&source, &source_variables, family_mask);
        let imported = destination.import(&source_family, &variable_map).unwrap();
        assert_eq!(
            imported.iter().collect::<Vec<_>>(),
            source_family.iter().collect::<Vec<_>>()
        );
    }
}

#[test]
fn explicit_import_validates_the_complete_bijective_ordered_map() {
    let source = FamilySpace::new(3).unwrap();
    let family = source.unit();
    let destination = FamilySpace::new(3).unwrap();
    let vars = variables(&destination, 3);

    assert!(matches!(
        destination.import(&family, &vars[..2]),
        Err(Error::InvalidVariableMapLength { .. })
    ));
    let larger = FamilySpace::new(4).unwrap();
    assert!(matches!(
        destination.import(&family, &[vars[0], vars[1], larger.variable(3).unwrap()]),
        Err(Error::InvalidMappedVariable { .. })
    ));
    assert!(matches!(
        destination.import(&family, &[vars[0], vars[0], vars[2]]),
        Err(Error::DuplicateMappedVariable { .. })
    ));
    assert!(matches!(
        destination.import(&family, &[vars[1], vars[0], vars[2]]),
        Err(Error::OrderMismatch { .. })
    ));
    let different_size = FamilySpace::new(2).unwrap();
    assert!(matches!(
        different_size.import(&family, &variables(&different_size, 2)),
        Err(Error::UniverseSizeMismatch { .. })
    ));
}

#[test]
fn multi_root_compaction_preserves_order_sharing_and_old_roots() {
    let space = FamilySpace::new(4).unwrap();
    let vars = variables(&space, 4);
    let left = space
        .from_sets([vec![vars[0], vars[2]], vec![vars[0], vars[2], vars[3]]])
        .unwrap();
    let right = space
        .from_sets([vec![vars[1], vars[2]], vec![vars[1], vars[2], vars[3]]])
        .unwrap();
    let expected_left = set_masks(&left);
    let expected_right = set_masks(&right);

    let (compacted_space, compacted) = space.compact(&[left.clone(), right.clone()]).unwrap();
    assert_eq!(set_masks(&compacted[0]), expected_left);
    assert_eq!(set_masks(&compacted[1]), expected_right);
    assert_eq!(set_masks(&left), expected_left);
    assert_eq!(set_masks(&right), expected_right);

    let baseline = FamilySpace::new(4).unwrap().stats().live_nodes;
    let combined_extra = compacted_space.stats().live_nodes - baseline;
    let first_destination = FamilySpace::new(4).unwrap();
    let first_map = variables(&first_destination, 4);
    first_destination.import(&left, &first_map).unwrap();
    let second_destination = FamilySpace::new(4).unwrap();
    let second_map = variables(&second_destination, 4);
    second_destination.import(&right, &second_map).unwrap();
    let separate_extra = (first_destination.stats().live_nodes - baseline)
        + (second_destination.stats().live_nodes - baseline);
    assert!(combined_extra < separate_extra);
}

#[test]
fn failed_import_keeps_the_source_valid_and_deep_compaction_is_iterative() {
    let source = FamilySpace::new(3).unwrap();
    let vars = variables(&source, 3);
    let family = source.from_sets([vec![vars[0], vars[2]]]).unwrap();
    let expected = set_masks(&family);
    let constrained = FamilySpace::builder(3)
        .limits(Limits {
            max_live_nodes: 6,
            ..Limits::default()
        })
        .build()
        .unwrap();
    let map = variables(&constrained, 3);
    assert!(matches!(
        constrained.import(&family, &map),
        Err(Error::LimitExceeded {
            kind: LimitKind::Node,
            ..
        })
    ));
    assert_eq!(set_masks(&family), expected);

    let deep = FamilySpace::new(4096).unwrap();
    let powerset = deep.powerset().unwrap();
    let (_, compacted) = deep.compact(&[powerset]).unwrap();
    assert_eq!(compacted[0].count(), BigUint::from(1u8) << 4096usize);
}
