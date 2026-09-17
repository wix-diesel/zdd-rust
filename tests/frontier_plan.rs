#![cfg(feature = "graph")]

use zdd_family::{BfsOrder, EdgeOrder, EdgeOrdering, FrontierPlan, Graph, GraphError, GraphSpace};

fn edge_indices(order: &EdgeOrder) -> Vec<usize> {
    order.as_slice().iter().map(|edge| edge.index()).collect()
}

#[test]
fn edge_order_rejects_missing_duplicate_and_out_of_range_edges() {
    let graph = Graph::from_edges(3, [(0, 1), (1, 2)]).unwrap();
    let edge0 = graph.edge_id(0).unwrap();
    let larger = Graph::from_edges(4, [(0, 1), (1, 2), (2, 3)]).unwrap();
    let foreign_edge = larger.edge_id(2).unwrap();

    assert!(matches!(
        EdgeOrder::new(&graph, [edge0]),
        Err(GraphError::InvalidEdgeOrderLength { .. })
    ));
    assert!(matches!(
        EdgeOrder::new(&graph, [edge0, edge0]),
        Err(GraphError::DuplicateOrderedEdge { .. })
    ));
    assert!(matches!(
        EdgeOrder::new(&graph, [edge0, foreign_edge]),
        Err(GraphError::InvalidEdge { .. })
    ));
}

#[test]
fn bfs_order_has_fixed_component_and_tie_breaking_rules() {
    let graph = Graph::from_edges(8, [(1, 2), (0, 1), (0, 3), (2, 3), (4, 5), (5, 7)]).unwrap();

    let order = BfsOrder.order(&graph).unwrap();
    assert_eq!(edge_indices(&order), vec![1, 2, 0, 3, 4, 5]);

    let space = GraphSpace::builder(&graph)
        .ordering(BfsOrder)
        .build()
        .unwrap();
    let fixed = (0..graph.edge_count())
        .map(|level| {
            let variable = space.as_family_space().variable(level).unwrap();
            space.edge_for_variable(variable).unwrap().index()
        })
        .collect::<Vec<_>>();
    assert_eq!(fixed, vec![1, 2, 0, 3, 4, 5]);
    let plan = FrontierPlan::new(&space).unwrap();
    assert_eq!(
        plan.steps()
            .map(|step| step.edge().index())
            .collect::<Vec<_>>(),
        fixed
    );
}

#[test]
fn bfs_order_handles_zero_edges_and_only_isolated_vertices() {
    for vertex_count in [0, 4] {
        let graph = Graph::from_edges(vertex_count, []).unwrap();
        assert!(BfsOrder.order(&graph).unwrap().as_slice().is_empty());
        let plan = FrontierPlan::new(&GraphSpace::new(&graph).unwrap()).unwrap();
        assert!(plan.is_empty());
        assert_eq!(plan.max_frontier_width(), 0);
        assert_eq!(plan.max_working_frontier_width(), 0);
    }
}

#[test]
fn frontier_plan_tracks_incidence_events_widths_and_stable_slots() {
    let graph = Graph::from_edges(6, [(0, 1), (1, 2), (0, 2), (3, 4)]).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    let plan = FrontierPlan::new(&space).unwrap();
    let vertices = (0..6)
        .map(|index| graph.vertex_id(index).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(plan.len(), 4);
    assert_eq!(plan.max_frontier_width(), 2);
    assert_eq!(plan.max_working_frontier_width(), 3);
    assert_eq!(plan.first_incident_step(vertices[0]).unwrap(), Some(0));
    assert_eq!(plan.last_incident_step(vertices[0]).unwrap(), Some(2));
    assert_eq!(plan.first_incident_step(vertices[5]).unwrap(), None);
    assert_eq!(plan.slot_for_vertex(vertices[5]).unwrap(), None);

    let step0 = plan.step(0).unwrap();
    assert_eq!(step0.introduced(), &[vertices[0], vertices[1]]);
    assert!(step0.forgotten().is_empty());
    assert_eq!(
        (
            step0.frontier_width_before(),
            step0.working_frontier_width(),
            step0.frontier_width_after()
        ),
        (0, 2, 2)
    );

    let step1 = plan.step(1).unwrap();
    assert_eq!(step1.introduced(), &[vertices[2]]);
    assert_eq!(step1.forgotten(), &[vertices[1]]);
    assert_eq!(step1.remaining_incident_edges(), [0, 1]);

    let step2 = plan.step(2).unwrap();
    assert!(step2.introduced().is_empty());
    assert_eq!(step2.forgotten(), &[vertices[0], vertices[2]]);

    let step3 = plan.step(3).unwrap();
    assert_eq!(step3.introduced(), &[vertices[3], vertices[4]]);
    assert_eq!(step3.forgotten(), &[vertices[3], vertices[4]]);
    assert_eq!(
        (
            step3.frontier_width_before(),
            step3.working_frontier_width(),
            step3.frontier_width_after()
        ),
        (0, 2, 0)
    );
    assert_eq!(step3.endpoint_slots().map(|slot| slot.index()), [0, 1]);
}

#[test]
fn frontier_plan_storage_events_are_linear_not_layer_snapshots() {
    const EDGE_COUNT: usize = 1_000;
    let graph = Graph::from_edges(EDGE_COUNT + 1, (0..EDGE_COUNT).map(|i| (i, i + 1))).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    let plan = FrontierPlan::new(&space).unwrap();

    assert_eq!(plan.len(), EDGE_COUNT);
    assert_eq!(
        plan.steps()
            .map(|step| step.introduced().len())
            .sum::<usize>(),
        EDGE_COUNT + 1
    );
    assert_eq!(
        plan.steps()
            .map(|step| step.forgotten().len())
            .sum::<usize>(),
        EDGE_COUNT + 1
    );
    assert_eq!(plan.max_frontier_width(), 1);
    assert_eq!(plan.max_working_frontier_width(), 2);
}
