#![cfg(feature = "graph")]

use std::collections::BTreeSet;

#[path = "../tools/performance-baseline/src/datasets.rs"]
mod datasets;

#[test]
fn performance_graph_corpus_is_deterministic_and_orders_every_edge() {
    let first = datasets::graph_datasets().unwrap();
    let second = datasets::graph_datasets().unwrap();
    let expected_names = BTreeSet::from([
        "chain",
        "tree",
        "ladder",
        "grid-thin",
        "grid-square",
        "complete",
        "sparse-seed-23",
    ]);

    assert_eq!(
        first
            .iter()
            .map(|dataset| dataset.name)
            .collect::<BTreeSet<_>>(),
        expected_names
    );
    for (left, right) in first.iter().zip(&second) {
        assert_eq!(left.graph.vertex_count(), right.graph.vertex_count());
        assert_eq!(left.graph.edge_count(), right.graph.edge_count());
        assert_eq!(left.good_order, right.good_order);
        assert_eq!(left.bad_order, right.bad_order);
        assert_order_is_permutation(left.good_order.as_slice(), left.graph.edge_count());
        assert_order_is_permutation(left.bad_order.as_slice(), left.graph.edge_count());
    }
}

fn assert_order_is_permutation(order: &[zdd_family::EdgeId], edge_count: usize) {
    assert_eq!(order.len(), edge_count);
    assert_eq!(
        order
            .iter()
            .map(|edge| edge.index())
            .collect::<BTreeSet<_>>(),
        (0..edge_count).collect()
    );
}
