use super::*;

#[test]
fn terminals_and_powerset_have_zdd_semantics() {
    let manager = ZddManager::new(3, 64, 0).unwrap();
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
    let manager = ZddManager::new(1, 16, 0).unwrap();
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
    let manager = ZddManager::new(10, 4_096, 0).unwrap();
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
    let manager = ZddManager::new(3, 64, 0).unwrap();
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

#[test]
fn normalized_snapshot_is_independent_of_backend_node_allocation() {
    let first = ZddManager::new(3, 64, 0).unwrap();
    let first_root = first
        .build_from_sets(&[vec![], vec![0], vec![0, 1], vec![2]])
        .unwrap()
        .0;

    let second = ZddManager::new(3, 64, 0).unwrap();
    let temporary = second
        .build_from_sets(&[vec![0, 2], vec![1], vec![1, 2]])
        .unwrap()
        .0;
    let second_root = second
        .build_from_sets(&[vec![2], vec![0, 1], vec![], vec![0]])
        .unwrap()
        .0;
    drop(temporary);

    let expected = concat!(
        "root=n0\n",
        "n0: variable=0, hi=n1, lo=n2\n",
        "n1: variable=1, hi=ONE, lo=ONE\n",
        "n2: variable=2, hi=ONE, lo=ONE",
    );
    assert_eq!(first.normalized_snapshot(&first_root), expected);
    assert_eq!(second.normalized_snapshot(&second_root), expected);
}
