use std::ops::ControlFlow;
use std::sync::{Arc, Barrier};
use std::thread;

use zdd_family::{
    BigUint, Error, FamilySpace, LimitKind, Limits, QueryError, QueryLimits, SetFamily, VariableId,
};

fn variables(space: &FamilySpace, count: usize) -> Vec<VariableId> {
    (0..count)
        .map(|index| space.variable(index).expect("index belongs to the space"))
        .collect()
}

fn masks(family: &SetFamily) -> Vec<u64> {
    family
        .iter()
        .map(|solution| {
            solution
                .iter()
                .fold(0_u64, |mask, variable| mask | (1 << variable.index()))
        })
        .collect()
}

#[test]
fn failed_operations_keep_existing_families_and_the_manager_reusable() {
    let space = FamilySpace::builder(2)
        .limits(Limits {
            max_live_nodes: 64,
            max_operation_memo_entries: 0,
            shared_cache_entries: 0,
            ..Limits::default()
        })
        .build()
        .unwrap();
    let variables = variables(&space, 2);
    let left = space.from_sets([vec![variables[0]]]).unwrap();
    let right = space.from_sets([vec![variables[1]]]).unwrap();
    let expected_left = masks(&left);
    let expected_right = masks(&right);

    let error = left.union(&right).unwrap_err();
    let Error::LimitExceeded { kind, stats, .. } = error else {
        panic!("operation memo limit must be reported");
    };
    assert_eq!(kind, LimitKind::OperationMemo);
    assert_eq!(stats.nodes_before, stats.nodes_after);
    assert_eq!(masks(&left), expected_left);
    assert_eq!(masks(&right), expected_right);

    // Terminal shortcuts need no operation memo and prove this manager remains usable.
    assert_eq!(masks(&left.union(&space.empty()).unwrap()), expected_left);
    assert_eq!(
        masks(&right.difference(&space.empty()).unwrap()),
        expected_right
    );
}

#[test]
fn query_limits_report_completed_work_without_affecting_later_queries() {
    let space = FamilySpace::new(3).unwrap();
    let family = space.powerset().unwrap();

    let error = family
        .count_index(&QueryLimits {
            max_snapshot_nodes: 2,
            max_total_count_bits: usize::MAX,
        })
        .unwrap_err();
    let QueryError::LimitExceeded { kind, stats, .. } = error else {
        panic!("query node limit must be reported");
    };
    assert_eq!(kind, LimitKind::QueryNodes);
    assert_eq!(stats.snapshot_nodes, 2);
    assert_eq!(stats.total_count_bits, 0);

    let index = family.count_index(&QueryLimits::default()).unwrap();
    assert_eq!(index.count(), &family.count());
}

#[test]
fn visitor_reentry_and_concurrent_same_space_reads_and_writes_complete() {
    let space = FamilySpace::new(5).unwrap();
    let family = space.powerset().unwrap();
    let visitor_space = space.clone();
    let mut visited = 0;
    let result = family.visit_solutions(|_| {
        // The traversal owns its snapshot, so user code can safely use the same manager.
        assert!(visitor_space.powerset().unwrap().contains(&[]).unwrap());
        visited += 1;
        if visited == 4 {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    assert_eq!(result, ControlFlow::Break(()));

    let start = Arc::new(Barrier::new(3));
    thread::scope(|scope| {
        let writer_space = space.clone();
        let writer_family = family.clone();
        let writer_start = Arc::clone(&start);
        let writer = scope.spawn(move || {
            writer_start.wait();
            for _ in 0..24 {
                let combined = writer_family.union(&writer_space.unit()).unwrap();
                assert!(combined.contains(&[]).unwrap());
            }
        });

        let reader_family = family.clone();
        let reader_start = Arc::clone(&start);
        let reader = scope.spawn(move || {
            reader_start.wait();
            for _ in 0..24 {
                let index = reader_family.count_index(&QueryLimits::default()).unwrap();
                assert_eq!(index.count(), &reader_family.count());
                assert_eq!(reader_family.iter().count(), 32);
            }
        });

        start.wait();
        writer.join().unwrap();
        reader.join().unwrap();
    });
}

#[test]
fn independent_spaces_can_be_used_concurrently() {
    thread::scope(|scope| {
        let first = scope.spawn(|| {
            let space = FamilySpace::new(6).unwrap();
            let family = space.powerset().unwrap();
            assert_eq!(
                family.count_index(&QueryLimits::default()).unwrap().count(),
                &BigUint::from(64_u8)
            );
        });
        let second = scope.spawn(|| {
            let space = FamilySpace::new(7).unwrap();
            let family = space.powerset().unwrap();
            assert_eq!(family.iter().count(), 128);
        });
        first.join().unwrap();
        second.join().unwrap();
    });
}
