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
