#![cfg(feature = "graph")]

use std::collections::BTreeSet;
use std::ops::ControlFlow;

use zdd_family::{BigUint, EdgeId, EdgeOrder, FamilySpace, Graph, GraphError, GraphSpace};

fn edge_indices(edges: &[EdgeId]) -> BTreeSet<usize> {
    edges.iter().map(|edge| edge.index()).collect()
}

fn solution_sets(family: &zdd_family::EdgeFamily) -> BTreeSet<BTreeSet<usize>> {
    family
        .iter()
        .map(|solution| edge_indices(&solution))
        .collect()
}

#[test]
fn graph_preserves_isolated_vertices_and_input_edge_ids() {
    let graph = Graph::from_edges(5, [(3, 1), (0, 2)]).unwrap();

    assert_eq!(graph.vertex_count(), 5);
    assert_eq!(graph.edge_count(), 2);
    assert_eq!(graph.vertex_id(4).unwrap().index(), 4);
    let edge = graph.edge_id(0).unwrap();
    let (first, second) = graph.endpoints(edge).unwrap();
    assert_eq!((first.index(), second.index()), (3, 1));
}

#[test]
fn graph_rejects_invalid_endpoints_self_loops_and_reversed_duplicates() {
    assert!(matches!(
        Graph::from_edges(2, [(0, 2)]),
        Err(GraphError::InvalidVertex { .. })
    ));
    assert!(matches!(
        Graph::from_edges(2, [(1, 1)]),
        Err(GraphError::SelfLoop { .. })
    ));
    assert!(matches!(
        Graph::from_edges(3, [(0, 2), (2, 0)]),
        Err(GraphError::DuplicateEdge { .. })
    ));
}

#[test]
fn graph_space_validates_complete_edge_orders() {
    let graph = Graph::from_edges(3, [(0, 1), (1, 2)]).unwrap();
    let edge0 = graph.edge_id(0).unwrap();

    assert!(matches!(
        EdgeOrder::new(&graph, [edge0]),
        Err(GraphError::InvalidEdgeOrderLength { .. })
    ));
    assert!(matches!(
        EdgeOrder::new(&graph, [edge0, edge0]),
        Err(GraphError::DuplicateOrderedEdge { .. })
    ));
}

#[test]
fn edge_for_variable_reports_a_variable_mapping_error() {
    let graph = Graph::from_edges(3, [(0, 1), (1, 2)]).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    let larger_space = FamilySpace::new(3).unwrap();
    let out_of_range = larger_space.variable(2).unwrap();

    let error = space.edge_for_variable(out_of_range).unwrap_err();
    assert!(matches!(
        error,
        GraphError::InvalidVariableMap {
            variable_index: 2,
            variable_count: 2,
            ..
        }
    ));
    assert_eq!(
        error.to_string(),
        "variable index 2 cannot be mapped in a graph space of 2 variables"
    );
}

#[test]
fn edge_families_restore_original_ids_when_level_order_differs() {
    let graph = Graph::from_edges(4, [(0, 1), (1, 2), (2, 3)]).unwrap();
    let edge0 = graph.edge_id(0).unwrap();
    let edge1 = graph.edge_id(1).unwrap();
    let edge2 = graph.edge_id(2).unwrap();
    let order = EdgeOrder::new(&graph, [edge2, edge0, edge1]).unwrap();
    let space = GraphSpace::builder(&graph).ordering(order).build().unwrap();

    assert_eq!(space.variable_for_edge(edge2).unwrap().index(), 0);
    assert_eq!(space.variable_for_edge(edge0).unwrap().index(), 1);
    assert_eq!(
        space
            .edge_for_variable(space.variable_for_edge(edge1).unwrap())
            .unwrap(),
        edge1
    );

    let family = space
        .from_edge_sets([vec![edge0, edge2], vec![edge1], vec![edge2, edge0]])
        .unwrap();
    assert_eq!(family.count(), BigUint::from(2u8));
    assert!(family.contains(&[edge2, edge0]).unwrap());
    assert_eq!(
        solution_sets(&family),
        BTreeSet::from([
            BTreeSet::from([edge1.index()]),
            BTreeSet::from([edge0.index(), edge2.index()]),
        ])
    );
    let ordered_pair = family.iter().find(|solution| solution.len() == 2).unwrap();
    assert_eq!(ordered_pair.as_slice(), &[edge2, edge0]);
}

#[test]
fn same_graph_space_composes_but_independent_spaces_do_not() {
    let graph = Graph::from_edges(3, [(0, 1), (1, 2)]).unwrap();
    let edge0 = graph.edge_id(0).unwrap();
    let edge1 = graph.edge_id(1).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    let clone = space.clone();
    let left = space.from_edge_sets([vec![edge0]]).unwrap();
    let right = clone.from_edge_sets([vec![edge1]]).unwrap();

    let union = left.union(&right).unwrap();
    assert_eq!(union.count(), BigUint::from(2u8));
    assert_eq!(
        union.cardinality().exactly(1).unwrap().count(),
        BigUint::from(2u8)
    );
    assert_eq!(
        union.filter_contains(edge1).unwrap().count(),
        BigUint::from(1u8)
    );

    let independent = GraphSpace::new(&graph)
        .unwrap()
        .from_edge_sets([vec![edge0]])
        .unwrap();
    assert!(matches!(
        left.union(&independent),
        Err(GraphError::ContextMismatch { .. })
    ));
}

#[test]
fn families_solutions_and_iterators_own_the_graph_mapping() {
    let (family, edge0, edge1) = {
        let graph = Graph::from_edges(3, [(0, 1), (1, 2)]).unwrap();
        let edge0 = graph.edge_id(0).unwrap();
        let edge1 = graph.edge_id(1).unwrap();
        let space = GraphSpace::new(&graph).unwrap();
        (
            space.from_edge_sets([vec![edge0, edge1]]).unwrap(),
            edge0,
            edge1,
        )
    };

    assert_eq!(family.graph().vertex_count(), 3);
    let mut visited = Vec::new();
    assert_eq!(
        family.visit_solutions::<()>(|solution| {
            visited.push(solution.to_vec());
            ControlFlow::Continue(())
        }),
        ControlFlow::Continue(())
    );
    assert_eq!(visited, vec![vec![edge0, edge1]]);

    let mut iterator = family.iter();
    drop(family);
    let solution = iterator.next().unwrap();
    drop(iterator);
    assert_eq!(solution.as_slice(), &[edge0, edge1]);
}

#[test]
fn zero_edge_graph_and_foreign_out_of_range_edge_are_handled() {
    let graph = Graph::from_edges(3, []).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    assert_eq!(space.powerset().unwrap().count(), BigUint::from(1u8));
    assert_eq!(space.unit().count(), BigUint::from(1u8));

    let larger = Graph::from_edges(3, [(0, 1), (1, 2)]).unwrap();
    let foreign_edge = larger.edge_id(1).unwrap();
    assert!(matches!(
        space.from_edge_sets([vec![foreign_edge]]),
        Err(GraphError::InvalidEdge { .. })
    ));
}

#[test]
fn graph_compaction_preserves_graph_order_edge_ids_and_input_order() {
    let graph = Graph::from_edges(4, [(0, 1), (1, 2), (2, 3)]).unwrap();
    let edge0 = graph.edge_id(0).unwrap();
    let edge1 = graph.edge_id(1).unwrap();
    let edge2 = graph.edge_id(2).unwrap();
    let order = EdgeOrder::new(&graph, [edge2, edge0, edge1]).unwrap();
    let space = GraphSpace::builder(&graph).ordering(order).build().unwrap();
    let first = space
        .from_edge_sets([vec![edge2], vec![edge0, edge1]])
        .unwrap();
    let second = space
        .from_edge_sets([vec![edge2, edge0], vec![edge1]])
        .unwrap();
    let expected_first = first.iter().collect::<Vec<_>>();
    let expected_second = second.iter().collect::<Vec<_>>();

    let (compacted_space, compacted) = space.compact(&[first.clone(), second.clone()]).unwrap();

    assert_eq!(compacted[0].iter().collect::<Vec<_>>(), expected_first);
    assert_eq!(compacted[1].iter().collect::<Vec<_>>(), expected_second);
    assert_eq!(first.iter().collect::<Vec<_>>(), expected_first);
    assert_eq!(compacted_space.variable_for_edge(edge2).unwrap().index(), 0);
    assert_eq!(compacted_space.variable_for_edge(edge0).unwrap().index(), 1);

    let foreign = GraphSpace::new(&graph).unwrap().unit();
    assert!(matches!(
        space.compact(&[first, foreign]),
        Err(GraphError::ContextMismatch { .. })
    ));
}
