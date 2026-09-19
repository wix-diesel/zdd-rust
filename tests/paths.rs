#![cfg(feature = "graph")]

use std::collections::{BTreeSet, VecDeque};

use zdd_family::{BigUint, BuildError, EdgeOrder, Graph, GraphError, GraphSpace};

type EdgeSet = BTreeSet<usize>;
type EdgeFamilySet = BTreeSet<EdgeSet>;

fn solution_sets(family: &zdd_family::EdgeFamily) -> EdgeFamilySet {
    family
        .iter()
        .map(|solution| solution.iter().map(|edge| edge.index()).collect())
        .collect()
}

fn path_oracle(graph: &Graph, source: usize, target: usize) -> EdgeFamilySet {
    assert!(graph.edge_count() < usize::BITS as usize);
    (0..(1usize << graph.edge_count()))
        .filter_map(|mask| {
            let mut degree = vec![0usize; graph.vertex_count()];
            let mut adjacency = vec![Vec::new(); graph.vertex_count()];
            let mut selected = EdgeSet::new();
            for index in 0..graph.edge_count() {
                if mask & (1 << index) == 0 {
                    continue;
                }
                let (first, second) = graph.endpoints(graph.edge_id(index).unwrap()).unwrap();
                degree[first.index()] += 1;
                degree[second.index()] += 1;
                adjacency[first.index()].push(second.index());
                adjacency[second.index()].push(first.index());
                selected.insert(index);
            }
            if degree[source] != 1 || degree[target] != 1 {
                return None;
            }
            if degree.iter().enumerate().any(|(vertex, &value)| {
                vertex != source && vertex != target && value != 0 && value != 2
            }) {
                return None;
            }

            let mut reached = vec![false; graph.vertex_count()];
            let mut queue = VecDeque::from([source]);
            reached[source] = true;
            while let Some(vertex) = queue.pop_front() {
                for &neighbor in &adjacency[vertex] {
                    if !reached[neighbor] {
                        reached[neighbor] = true;
                        queue.push_back(neighbor);
                    }
                }
            }
            let all_selected_connected = degree
                .iter()
                .enumerate()
                .all(|(vertex, &value)| value == 0 || reached[vertex]);
            (reached[target] && all_selected_connected).then_some(selected)
        })
        .collect()
}

#[test]
fn paths_equal_an_independent_exhaustive_oracle_on_small_graphs() {
    for vertex_count in 2..=4 {
        let candidate_edges: Vec<_> = (0..vertex_count)
            .flat_map(|first| ((first + 1)..vertex_count).map(move |second| (first, second)))
            .collect();
        for graph_mask in 0..(1usize << candidate_edges.len()) {
            let edges = candidate_edges
                .iter()
                .enumerate()
                .filter_map(|(index, &edge)| (graph_mask & (1 << index) != 0).then_some(edge));
            let graph = Graph::from_edges(vertex_count, edges).unwrap();
            let space = GraphSpace::new(&graph).unwrap();
            for source in 0..vertex_count {
                for target in 0..vertex_count {
                    if source == target {
                        continue;
                    }
                    let actual = space
                        .paths(
                            graph.vertex_id(source).unwrap(),
                            graph.vertex_id(target).unwrap(),
                        )
                        .unwrap();
                    assert_eq!(
                        solution_sets(&actual),
                        path_oracle(&graph, source, target),
                        "vertex_count={vertex_count}, graph_mask={graph_mask}, source={source}, target={target}"
                    );
                }
            }
        }
    }
}

#[test]
fn endpoint_validation_and_no_solution_cases_are_distinguished() {
    let graph = Graph::from_edges(5, [(0, 1), (1, 2), (3, 4)]).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    let vertex0 = graph.vertex_id(0).unwrap();
    let vertex2 = graph.vertex_id(2).unwrap();
    let vertex3 = graph.vertex_id(3).unwrap();

    let same = match space.paths(vertex0, vertex0) {
        Ok(_) => panic!("identical endpoints unexpectedly succeeded"),
        Err(error) => error,
    };
    assert!(matches!(
        same,
        BuildError::Problem {
            source: GraphError::IdenticalPathEndpoints { vertex: 0, .. },
            ..
        }
    ));

    let larger = Graph::from_edges(6, []).unwrap();
    let out_of_range = match space.paths(vertex0, larger.vertex_id(5).unwrap()) {
        Ok(_) => panic!("out-of-range endpoint unexpectedly succeeded"),
        Err(error) => error,
    };
    assert!(matches!(
        out_of_range,
        BuildError::Problem {
            source: GraphError::InvalidVertex { index: 5, .. },
            ..
        }
    ));

    assert!(space.paths(vertex0, vertex3).unwrap().is_empty());
    assert!(!space.paths(vertex0, vertex2).unwrap().is_empty());

    let isolated = Graph::from_edges(4, [(0, 1)]).unwrap();
    let isolated_space = GraphSpace::new(&isolated).unwrap();
    assert!(
        isolated_space
            .paths(
                isolated.vertex_id(0).unwrap(),
                isolated.vertex_id(3).unwrap()
            )
            .unwrap()
            .is_empty()
    );
}

#[test]
fn early_forgotten_endpoints_and_extra_components_work_across_orderings() {
    let graph =
        Graph::from_edges(7, [(0, 1), (1, 2), (3, 4), (2, 5), (4, 5), (5, 6), (2, 6)]).unwrap();
    let source = graph.vertex_id(0).unwrap();
    let target = graph.vertex_id(6).unwrap();
    let expected = path_oracle(&graph, 0, 6);

    let input = GraphSpace::new(&graph).unwrap();
    assert_eq!(
        solution_sets(&input.paths(source, target).unwrap()),
        expected
    );

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
        .unwrap();
    assert_eq!(
        solution_sets(&reversed.paths(source, target).unwrap()),
        expected
    );
}

#[test]
fn path_family_composes_with_required_edges_length_filters_count_and_iteration() {
    let graph = Graph::from_edges(5, [(0, 1), (1, 4), (0, 2), (2, 3), (3, 4), (1, 2)]).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    let paths = space
        .paths(graph.vertex_id(0).unwrap(), graph.vertex_id(4).unwrap())
        .unwrap();
    let required = graph.edge_id(2).unwrap();
    let short_with_required = paths
        .cardinality()
        .at_most(3)
        .unwrap()
        .filter_contains(required)
        .unwrap();

    assert_eq!(short_with_required.count(), BigUint::from(2u8));
    assert_eq!(
        solution_sets(&short_with_required),
        EdgeFamilySet::from([EdgeSet::from([1, 2, 5]), EdgeSet::from([2, 3, 4])])
    );
    assert!(
        paths
            .contains(&[graph.edge_id(0).unwrap(), graph.edge_id(1).unwrap()])
            .unwrap()
    );
    assert_eq!(paths.count(), BigUint::from(solution_sets(&paths).len()));
}
