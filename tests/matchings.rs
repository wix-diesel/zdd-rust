#![cfg(feature = "graph")]

use std::collections::BTreeSet;

use zdd_family::{BfsOrder, BigUint, EdgeId, EdgeOrder, Graph, GraphSpace};

type EdgeSet = BTreeSet<usize>;
type EdgeFamilySet = BTreeSet<EdgeSet>;

fn solution_sets(family: &zdd_family::EdgeFamily) -> EdgeFamilySet {
    family
        .iter()
        .map(|solution| solution.iter().map(|edge| edge.index()).collect())
        .collect()
}

fn matching_oracle(graph: &Graph) -> EdgeFamilySet {
    assert!(graph.edge_count() < usize::BITS as usize);
    (0..(1usize << graph.edge_count()))
        .filter_map(|mask| {
            let mut used = vec![false; graph.vertex_count()];
            let mut edges = EdgeSet::new();
            for index in 0..graph.edge_count() {
                if mask & (1 << index) == 0 {
                    continue;
                }
                let edge = graph.edge_id(index).unwrap();
                let (first, second) = graph.endpoints(edge).unwrap();
                if used[first.index()] || used[second.index()] {
                    return None;
                }
                used[first.index()] = true;
                used[second.index()] = true;
                edges.insert(index);
            }
            Some(edges)
        })
        .collect()
}

fn complete_graph(vertex_count: usize) -> Graph {
    Graph::from_edges(
        vertex_count,
        (0..vertex_count)
            .flat_map(|first| ((first + 1)..vertex_count).map(move |second| (first, second))),
    )
    .unwrap()
}

#[test]
fn matchings_equal_an_independent_exhaustive_oracle_on_small_graphs() {
    for vertex_count in 0..=4 {
        let candidate_edges: Vec<_> = (0..vertex_count)
            .flat_map(|first| ((first + 1)..vertex_count).map(move |second| (first, second)))
            .collect();
        for graph_mask in 0..(1usize << candidate_edges.len()) {
            let edges = candidate_edges
                .iter()
                .enumerate()
                .filter_map(|(index, &edge)| (graph_mask & (1 << index) != 0).then_some(edge));
            let graph = Graph::from_edges(vertex_count, edges).unwrap();
            let actual = GraphSpace::new(&graph).unwrap().matchings().unwrap();
            assert_eq!(
                solution_sets(&actual),
                matching_oracle(&graph),
                "vertex_count={vertex_count}, graph_mask={graph_mask}, edges={candidate_edges:?}"
            );
        }
    }

    let complete_five = complete_graph(5);
    let actual = GraphSpace::new(&complete_five)
        .unwrap()
        .matchings()
        .unwrap();
    assert_eq!(solution_sets(&actual), matching_oracle(&complete_five));
}

#[test]
fn zero_edge_graph_is_unit_and_isolated_vertices_do_not_remove_solutions() {
    for vertex_count in [0, 1, 7] {
        let graph = Graph::from_edges(vertex_count, []).unwrap();
        let family = GraphSpace::new(&graph).unwrap().matchings().unwrap();
        assert_eq!(family.count(), BigUint::from(1u8));
        assert_eq!(
            solution_sets(&family),
            EdgeFamilySet::from([EdgeSet::new()])
        );
    }

    let graph = Graph::from_edges(8, [(1, 2), (2, 3)]).unwrap();
    let family = GraphSpace::new(&graph).unwrap().matchings().unwrap();
    assert_eq!(solution_sets(&family), matching_oracle(&graph));
}

#[test]
fn original_edge_sets_are_identical_across_orderings() {
    let graph =
        Graph::from_edges(6, [(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (0, 5), (1, 4)]).unwrap();
    let input = GraphSpace::new(&graph).unwrap().matchings().unwrap();
    let bfs = GraphSpace::builder(&graph)
        .ordering(BfsOrder)
        .build()
        .unwrap()
        .matchings()
        .unwrap();
    let reverse = EdgeOrder::new(
        &graph,
        (0..graph.edge_count())
            .rev()
            .map(|index| graph.edge_id(index).unwrap()),
    )
    .unwrap();
    let reversed = GraphSpace::builder(&graph)
        .ordering(reverse)
        .build()
        .unwrap()
        .matchings()
        .unwrap();

    let expected = matching_oracle(&graph);
    assert_eq!(solution_sets(&input), expected);
    assert_eq!(solution_sets(&bfs), expected);
    assert_eq!(solution_sets(&reversed), expected);
}

#[test]
fn matching_size_filters_and_same_space_set_operations_compose() {
    let graph = Graph::from_edges(5, [(0, 1), (1, 2), (2, 3), (3, 4)]).unwrap();
    let edges: Vec<EdgeId> = (0..graph.edge_count())
        .map(|index| graph.edge_id(index).unwrap())
        .collect();
    let space = GraphSpace::new(&graph).unwrap();
    let matchings = space.matchings().unwrap();
    let pairs = matchings.cardinality().exactly(2).unwrap();

    assert_eq!(
        solution_sets(&pairs),
        EdgeFamilySet::from([
            EdgeSet::from([0, 2]),
            EdgeSet::from([0, 3]),
            EdgeSet::from([1, 3]),
        ])
    );

    let required = space.from_edge_sets([[edges[0], edges[3]]]).unwrap();
    assert!(required.is_subset_of(&matchings).unwrap());
    assert!(
        required
            .intersection(&pairs)
            .unwrap()
            .equivalent(&required)
            .unwrap()
    );
    assert_eq!(
        pairs.union(&space.unit()).unwrap().count(),
        BigUint::from(4u8)
    );
    assert!(
        matchings
            .intersection(&space.powerset().unwrap())
            .unwrap()
            .equivalent(&matchings)
            .unwrap()
    );
}
