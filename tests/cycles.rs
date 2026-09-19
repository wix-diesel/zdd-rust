#![cfg(feature = "graph")]

use std::collections::{BTreeSet, VecDeque};

use zdd_family::{BfsOrder, BigUint, EdgeOrder, Graph, GraphSpace};

type EdgeSet = BTreeSet<usize>;
type EdgeFamilySet = BTreeSet<EdgeSet>;

fn solution_sets(family: &zdd_family::EdgeFamily) -> EdgeFamilySet {
    family
        .iter()
        .map(|solution| solution.iter().map(|edge| edge.index()).collect())
        .collect()
}

fn cycle_oracle(graph: &Graph) -> EdgeFamilySet {
    assert!(graph.edge_count() < usize::BITS as usize);
    (1..(1usize << graph.edge_count()))
        .filter_map(|mask| {
            let mut degree = vec![0usize; graph.vertex_count()];
            let mut adjacency = vec![Vec::new(); graph.vertex_count()];
            let mut selected = EdgeSet::new();
            for index in 0..graph.edge_count() {
                if mask & (1 << index) == 0 {
                    continue;
                }
                let edge = graph.edge_id(index).unwrap();
                let (first, second) = graph.endpoints(edge).unwrap();
                degree[first.index()] += 1;
                degree[second.index()] += 1;
                adjacency[first.index()].push(second.index());
                adjacency[second.index()].push(first.index());
                selected.insert(index);
            }
            if degree.iter().any(|&value| value != 0 && value != 2) {
                return None;
            }

            let start = degree.iter().position(|&value| value != 0).unwrap();
            let mut reached = vec![false; graph.vertex_count()];
            let mut queue = VecDeque::from([start]);
            reached[start] = true;
            while let Some(vertex) = queue.pop_front() {
                for &neighbor in &adjacency[vertex] {
                    if !reached[neighbor] {
                        reached[neighbor] = true;
                        queue.push_back(neighbor);
                    }
                }
            }
            degree
                .iter()
                .enumerate()
                .all(|(vertex, &value)| value == 0 || reached[vertex])
                .then_some(selected)
        })
        .collect()
}

#[test]
fn cycles_equal_an_independent_exhaustive_oracle_on_small_graphs() {
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
            let actual = GraphSpace::new(&graph).unwrap().cycles().unwrap();
            assert_eq!(
                solution_sets(&actual),
                cycle_oracle(&graph),
                "vertex_count={vertex_count}, graph_mask={graph_mask}"
            );
        }
    }
}

#[test]
fn empty_multiple_branched_and_simultaneously_forgotten_cases_are_handled() {
    for graph in [
        Graph::from_edges(0, []).unwrap(),
        Graph::from_edges(5, [(0, 1), (1, 2), (2, 3), (3, 4)]).unwrap(),
    ] {
        assert!(
            GraphSpace::new(&graph)
                .unwrap()
                .cycles()
                .unwrap()
                .is_empty()
        );
    }

    let disconnected =
        Graph::from_edges(7, [(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 6)]).unwrap();
    let cycles = GraphSpace::new(&disconnected).unwrap().cycles().unwrap();
    assert_eq!(solution_sets(&cycles), cycle_oracle(&disconnected));
    assert_eq!(cycles.count(), BigUint::from(2u8));
    assert!(!cycles.contains(&[]).unwrap());
    assert!(
        !cycles
            .contains(
                &(0..6)
                    .map(|index| disconnected.edge_id(index).unwrap())
                    .collect::<Vec<_>>()
            )
            .unwrap()
    );

    let simultaneous = Graph::from_edges(3, [(0, 1), (0, 2), (1, 2)]).unwrap();
    assert_eq!(
        solution_sets(&GraphSpace::new(&simultaneous).unwrap().cycles().unwrap()),
        EdgeFamilySet::from([EdgeSet::from([0, 1, 2])])
    );
}

#[test]
fn original_edge_sets_are_identical_across_orderings() {
    let graph = Graph::from_edges(
        6,
        [
            (0, 1),
            (1, 2),
            (2, 0),
            (2, 3),
            (3, 4),
            (4, 5),
            (5, 2),
            (1, 4),
        ],
    )
    .unwrap();
    let input = GraphSpace::new(&graph).unwrap().cycles().unwrap();
    let bfs = GraphSpace::builder(&graph)
        .ordering(BfsOrder)
        .build()
        .unwrap()
        .cycles()
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
        .cycles()
        .unwrap();

    let expected = cycle_oracle(&graph);
    assert_eq!(solution_sets(&input), expected);
    assert_eq!(solution_sets(&bfs), expected);
    assert_eq!(solution_sets(&reversed), expected);
}

#[test]
fn cycle_family_composes_with_cardinality_and_inclusion_filters() {
    let graph =
        Graph::from_edges(5, [(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2), (1, 3)]).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    let cycles = space.cycles().unwrap();
    let required = graph.edge_id(0).unwrap();
    let triangles_with_required = cycles
        .cardinality()
        .exactly(3)
        .unwrap()
        .filter_contains(required)
        .unwrap();
    let expected: EdgeFamilySet = cycle_oracle(&graph)
        .into_iter()
        .filter(|edges| edges.len() == 3 && edges.contains(&required.index()))
        .collect();

    assert_eq!(solution_sets(&triangles_with_required), expected);
    assert_eq!(
        triangles_with_required.count(),
        BigUint::from(expected.len())
    );
    assert!(triangles_with_required.is_subset_of(&cycles).unwrap());
}
