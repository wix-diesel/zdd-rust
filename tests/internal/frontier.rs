use std::collections::BTreeSet;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

use crate::{
    Branch, Choice, EdgeOrder, EdgeStep, FrontierBuilder, FrontierPlan, FrontierProblem,
    FrontierView, Graph, GraphSpace,
};

use super::cycles::{CycleProblem, CycleSlot, CycleState};
use super::matchings::{MatchingProblem, MatchingState};
use super::paths::{PathProblem, PathSlot, PathState};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct NoMergeState<S> {
    inner: S,
    prefix: Vec<Choice>,
}

#[derive(Clone, Debug)]
struct NoMerge<P>(P);

impl<P: FrontierProblem> FrontierProblem for NoMerge<P> {
    type State = NoMergeState<P::State>;
    type Error = P::Error;

    fn initial_state(&self, graph: &Graph) -> Result<Self::State, Self::Error> {
        Ok(NoMergeState {
            inner: self.0.initial_state(graph)?,
            prefix: Vec::new(),
        })
    }

    fn transition(
        &self,
        state: &mut Self::State,
        step: &EdgeStep<'_>,
        choice: Choice,
    ) -> Result<Branch, Self::Error> {
        let branch = self.0.transition(&mut state.inner, step, choice)?;
        if branch == Branch::Keep {
            state.prefix.push(choice);
        }
        Ok(branch)
    }

    fn canonicalize(&self, state: &mut Self::State, next: &FrontierView<'_>) {
        self.0.canonicalize(&mut state.inner, next);
    }

    fn finalize(&self, state: &Self::State) -> Result<bool, Self::Error> {
        self.0.finalize(&state.inner)
    }
}

#[derive(Clone, Debug)]
struct ExhaustiveProblem<F> {
    accepts: F,
}

impl<F: Fn(u64) -> bool> FrontierProblem for ExhaustiveProblem<F> {
    type State = u64;
    type Error = std::convert::Infallible;

    fn initial_state(&self, _graph: &Graph) -> Result<Self::State, Self::Error> {
        Ok(0)
    }

    fn transition(
        &self,
        selected: &mut Self::State,
        step: &EdgeStep<'_>,
        choice: Choice,
    ) -> Result<Branch, Self::Error> {
        if choice == Choice::Include {
            *selected |= 1 << step.edge().index();
        }
        Ok(Branch::Keep)
    }

    fn canonicalize(&self, _state: &mut Self::State, _next: &FrontierView<'_>) {}

    fn finalize(&self, state: &Self::State) -> Result<bool, Self::Error> {
        Ok((self.accepts)(*state))
    }
}

fn original_mask(space: &GraphSpace, variable_mask: u64) -> u64 {
    (0..space.graph().edge_count()).fold(0, |mask, level| {
        if variable_mask & (1 << level) == 0 {
            return mask;
        }
        let variable = space.as_family_space().variable(level).unwrap();
        let edge = space.edge_for_variable(variable).unwrap();
        mask | (1 << edge.index())
    })
}

fn family_masks(family: &crate::EdgeFamily) -> BTreeSet<u64> {
    family
        .iter()
        .map(|solution| {
            solution
                .iter()
                .fold(0, |mask, edge| mask | (1 << edge.index()))
        })
        .collect()
}

fn oracle_masks(graph: &Graph, accepts: impl Fn(u64) -> bool) -> BTreeSet<u64> {
    (0..(1u64 << graph.edge_count()))
        .filter(|&mask| accepts(mask))
        .collect()
}

fn matching_accepts(graph: &Graph, mask: u64) -> bool {
    let mut matched = vec![false; graph.vertex_count()];
    for index in 0..graph.edge_count() {
        if mask & (1 << index) == 0 {
            continue;
        }
        let (first, second) = graph.endpoints(graph.edge_id(index).unwrap()).unwrap();
        if matched[first.index()] || matched[second.index()] {
            return false;
        }
        matched[first.index()] = true;
        matched[second.index()] = true;
    }
    true
}

fn path_accepts(graph: &Graph, source: usize, target: usize, mask: u64) -> bool {
    let mut degree = vec![0usize; graph.vertex_count()];
    let mut adjacency = vec![Vec::new(); graph.vertex_count()];
    for index in 0..graph.edge_count() {
        if mask & (1 << index) == 0 {
            continue;
        }
        let (first, second) = graph.endpoints(graph.edge_id(index).unwrap()).unwrap();
        degree[first.index()] += 1;
        degree[second.index()] += 1;
        adjacency[first.index()].push(second.index());
        adjacency[second.index()].push(first.index());
    }
    if degree[source] != 1 || degree[target] != 1 {
        return false;
    }
    if degree
        .iter()
        .enumerate()
        .any(|(vertex, &value)| vertex != source && vertex != target && value != 0 && value != 2)
    {
        return false;
    }
    selected_vertices_are_connected(source, &degree, &adjacency)
}

fn cycle_accepts(graph: &Graph, mask: u64) -> bool {
    if mask == 0 {
        return false;
    }
    let mut degree = vec![0usize; graph.vertex_count()];
    let mut adjacency = vec![Vec::new(); graph.vertex_count()];
    for index in 0..graph.edge_count() {
        if mask & (1 << index) == 0 {
            continue;
        }
        let (first, second) = graph.endpoints(graph.edge_id(index).unwrap()).unwrap();
        degree[first.index()] += 1;
        degree[second.index()] += 1;
        adjacency[first.index()].push(second.index());
        adjacency[second.index()].push(first.index());
    }
    if degree.iter().any(|&value| value != 0 && value != 2) {
        return false;
    }
    let start = degree.iter().position(|&value| value != 0).unwrap();
    selected_vertices_are_connected(start, &degree, &adjacency)
}

fn selected_vertices_are_connected(
    start: usize,
    degree: &[usize],
    adjacency: &[Vec<usize>],
) -> bool {
    let mut stack = vec![start];
    let mut reached = vec![false; degree.len()];
    reached[start] = true;
    while let Some(vertex) = stack.pop() {
        for &neighbor in &adjacency[vertex] {
            if !reached[neighbor] {
                reached[neighbor] = true;
                stack.push(neighbor);
            }
        }
    }
    degree
        .iter()
        .enumerate()
        .all(|(vertex, &value)| value == 0 || reached[vertex])
}

fn assert_build_modes<P, F>(space: &GraphSpace, problem: P, accepts: F, expected: &BTreeSet<u64>)
where
    P: FrontierProblem + Clone,
    P::State: Debug,
    P::Error: Debug,
    F: Fn(u64) -> bool + Clone,
{
    let merged_pruned = FrontierBuilder::new(space).build(problem.clone()).unwrap();
    let unmerged_pruned = FrontierBuilder::new(space).build(NoMerge(problem)).unwrap();
    let merged_unpruned = FrontierBuilder::new(space)
        .build(ExhaustiveProblem {
            accepts: accepts.clone(),
        })
        .unwrap();
    let unmerged_unpruned = FrontierBuilder::new(space)
        .build(NoMerge(ExhaustiveProblem { accepts }))
        .unwrap();

    for actual in [
        merged_pruned,
        unmerged_pruned,
        merged_unpruned,
        unmerged_unpruned,
    ] {
        assert_eq!(&family_masks(&actual), expected);
    }
}

fn advance_frontier(
    plan: &FrontierPlan,
    step: EdgeStep<'_>,
    active_slots: &mut [Option<crate::VertexId>],
) {
    for &vertex in step.introduced() {
        let slot = plan.slot_for_vertex(vertex).unwrap().unwrap();
        active_slots[slot.index()] = Some(vertex);
    }
    for &vertex in step.forgotten() {
        let slot = plan.slot_for_vertex(vertex).unwrap().unwrap();
        active_slots[slot.index()] = None;
    }
}

fn accepted_suffixes(
    space: &GraphSpace,
    prefix: u64,
    layer: usize,
    accepts: &impl Fn(u64) -> bool,
) -> BTreeSet<u64> {
    let remaining = space.graph().edge_count() - layer;
    (0..(1u64 << remaining))
        .filter(|&suffix| {
            let variable_mask = prefix | (suffix << layer);
            accepts(original_mask(space, variable_mask))
        })
        .collect()
}

fn assert_equal_states_have_equal_suffixes<P>(
    space: &GraphSpace,
    problem: P,
    accepts: impl Fn(u64) -> bool,
) where
    P: FrontierProblem,
    P::State: Debug,
    P::Error: Debug,
{
    let plan = FrontierPlan::new(space).unwrap();
    let mut active_slots = vec![None; plan.max_working_frontier_width()];
    let empty_view = FrontierView::new(&active_slots, 0);
    let mut initial = problem.initial_state(space.graph()).unwrap();
    problem.canonicalize(&mut initial, &empty_view);
    let mut prefixes = vec![(0u64, initial)];
    let mut equal_pair_count = 0usize;

    for (layer, step) in plan.steps().enumerate() {
        advance_frontier(&plan, step, &mut active_slots);
        let next_view = FrontierView::new(&active_slots, step.frontier_width_after());
        let mut next = Vec::new();
        for (prefix, state) in &prefixes {
            for choice in [Choice::Exclude, Choice::Include] {
                let mut candidate = state.clone();
                if problem.transition(&mut candidate, &step, choice).unwrap() == Branch::Reject {
                    continue;
                }
                problem.canonicalize(&mut candidate, &next_view);
                let selected = u64::from(choice == Choice::Include) << layer;
                next.push((prefix | selected, candidate));
            }
        }
        prefixes = next;

        for first in 0..prefixes.len() {
            for second in (first + 1)..prefixes.len() {
                if prefixes[first].1 != prefixes[second].1 {
                    continue;
                }
                equal_pair_count += 1;
                let first_suffixes =
                    accepted_suffixes(space, prefixes[first].0, layer + 1, &accepts);
                let second_suffixes =
                    accepted_suffixes(space, prefixes[second].0, layer + 1, &accepts);
                assert_eq!(
                    first_suffixes,
                    second_suffixes,
                    "equal canonical states had different suffix languages at layer {}: {:?}",
                    layer + 1,
                    prefixes[first].1
                );
            }
        }
    }

    assert!(
        equal_pair_count > 0,
        "fixture did not exercise state merging"
    );
}

fn complete_graph(vertex_count: usize) -> Graph {
    Graph::from_edges(
        vertex_count,
        (0..vertex_count)
            .flat_map(|first| ((first + 1)..vertex_count).map(move |second| (first, second))),
    )
    .unwrap()
}

fn fixed_seed_order(graph: &Graph) -> EdgeOrder {
    let mut edges: Vec<_> = (0..graph.edge_count())
        .map(|index| graph.edge_id(index).unwrap())
        .collect();
    let mut state = 0x6a09_e667_f3bc_c909u64;
    for upper in (1..edges.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        edges.swap(upper, state as usize % (upper + 1));
    }
    EdgeOrder::new(graph, edges).unwrap()
}

fn representative_spaces(graph: &Graph) -> Vec<GraphSpace> {
    let reverse = EdgeOrder::new(
        graph,
        (0..graph.edge_count())
            .rev()
            .map(|index| graph.edge_id(index).unwrap()),
    )
    .unwrap();
    vec![
        GraphSpace::new(graph).unwrap(),
        GraphSpace::builder(graph)
            .ordering(crate::BfsOrder)
            .build()
            .unwrap(),
        GraphSpace::builder(graph)
            .ordering(reverse)
            .build()
            .unwrap(),
        GraphSpace::builder(graph)
            .ordering(fixed_seed_order(graph))
            .build()
            .unwrap(),
    ]
}

fn permutations(values: &mut [usize], start: usize, output: &mut Vec<Vec<usize>>) {
    if start == values.len() {
        output.push(values.to_vec());
        return;
    }
    for index in start..values.len() {
        values.swap(start, index);
        permutations(values, start + 1, output);
        values.swap(start, index);
    }
}

fn assert_all_problem_modes(space: &GraphSpace) {
    let graph = space.graph();
    let matching = oracle_masks(graph, |mask| matching_accepts(graph, mask));
    let cycles = oracle_masks(graph, |mask| cycle_accepts(graph, mask));
    let paths = oracle_masks(graph, |mask| path_accepts(graph, 0, 3, mask));
    assert_build_modes(
        space,
        MatchingProblem,
        |mask| matching_accepts(graph, mask),
        &matching,
    );
    assert_build_modes(
        space,
        CycleProblem,
        |mask| cycle_accepts(graph, mask),
        &cycles,
    );
    assert_build_modes(
        space,
        PathProblem::new(graph.vertex_id(0).unwrap(), graph.vertex_id(3).unwrap()),
        |mask| path_accepts(graph, 0, 3, mask),
        &paths,
    );
}

#[test]
fn canonical_state_suffix_languages_match_independent_oracles() {
    let graph = complete_graph(4);
    for space in representative_spaces(&graph) {
        assert_equal_states_have_equal_suffixes(&space, MatchingProblem, |mask| {
            matching_accepts(&graph, mask)
        });
        assert_equal_states_have_equal_suffixes(&space, CycleProblem, |mask| {
            cycle_accepts(&graph, mask)
        });
        assert_equal_states_have_equal_suffixes(
            &space,
            PathProblem::new(graph.vertex_id(0).unwrap(), graph.vertex_id(3).unwrap()),
            |mask| path_accepts(&graph, 0, 3, mask),
        );
    }
}

#[test]
fn merge_pruning_and_representative_orders_preserve_complete_solution_sets() {
    let graph = complete_graph(4);
    for space in representative_spaces(&graph) {
        assert_all_problem_modes(&space);
    }
}

#[test]
fn every_small_edge_permutation_preserves_complete_solution_sets() {
    let graph = Graph::from_edges(4, [(0, 1), (1, 2), (2, 3), (3, 0)]).unwrap();
    let mut orders = Vec::new();
    permutations(&mut [0, 1, 2, 3], 0, &mut orders);
    for order in orders {
        let edge_order = EdgeOrder::new(
            &graph,
            order.into_iter().map(|index| graph.edge_id(index).unwrap()),
        )
        .unwrap();
        let space = GraphSpace::builder(&graph)
            .ordering(edge_order)
            .build()
            .unwrap();
        assert_all_problem_modes(&space);
    }
}

#[test]
fn canonicalization_is_idempotent_and_renames_partitions_with_metadata() {
    let graph = Graph::from_edges(3, [(0, 1), (1, 2)]).unwrap();
    let vertices = [
        Some(graph.vertex_id(0).unwrap()),
        Some(graph.vertex_id(1).unwrap()),
        Some(graph.vertex_id(2).unwrap()),
    ];
    let view = FrontierView::new(&vertices, 3);

    let path_problem = PathProblem::new(vertices[0].unwrap(), vertices[2].unwrap());
    let mut path = PathState {
        slots: vec![
            PathSlot {
                degree: 1,
                component: Some(7),
                endpoint_mask: 1,
            },
            PathSlot {
                degree: 2,
                component: Some(4),
                endpoint_mask: 0,
            },
            PathSlot {
                degree: 1,
                component: Some(7),
                endpoint_mask: 1,
            },
        ],
        done: false,
        possible: true,
    };
    let mut renamed_path = path.clone();
    renamed_path.slots[0].component = Some(91);
    renamed_path.slots[1].component = Some(12);
    renamed_path.slots[2].component = Some(91);
    path_problem.canonicalize(&mut path, &view);
    path_problem.canonicalize(&mut renamed_path, &view);
    assert_eq!(path, renamed_path);
    let once = path.clone();
    path_problem.canonicalize(&mut path, &view);
    assert_eq!(path, once);
    let mut different_metadata = once.clone();
    different_metadata.slots[2].endpoint_mask = 3;
    path_problem.canonicalize(&mut different_metadata, &view);
    assert_ne!(different_metadata, once);

    let mut cycle = CycleState {
        slots: vec![
            CycleSlot {
                degree: 1,
                component: Some(8),
            },
            CycleSlot {
                degree: 2,
                component: Some(3),
            },
            CycleSlot {
                degree: 1,
                component: Some(8),
            },
        ],
        done: false,
    };
    let mut renamed_cycle = cycle.clone();
    renamed_cycle.slots[0].component = Some(44);
    renamed_cycle.slots[1].component = Some(2);
    renamed_cycle.slots[2].component = Some(44);
    CycleProblem.canonicalize(&mut cycle, &view);
    CycleProblem.canonicalize(&mut renamed_cycle, &view);
    assert_eq!(cycle, renamed_cycle);
    let once = cycle.clone();
    CycleProblem.canonicalize(&mut cycle, &view);
    assert_eq!(cycle, once);
    let mut completed = once.clone();
    completed.done = true;
    assert_ne!(completed, once);

    let mut matching = MatchingState {
        matched_slots: vec![true, false, true],
    };
    MatchingProblem.canonicalize(&mut matching, &view);
    let once = matching.clone();
    MatchingProblem.canonicalize(&mut matching, &view);
    assert_eq!(matching, once);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConstantHashState;

impl Hash for ConstantHashState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        0u8.hash(state);
    }
}

#[derive(Clone, Copy, Debug)]
struct LayerAgnosticProblem;

impl FrontierProblem for LayerAgnosticProblem {
    type State = ConstantHashState;
    type Error = std::convert::Infallible;

    fn initial_state(&self, _graph: &Graph) -> Result<Self::State, Self::Error> {
        Ok(ConstantHashState)
    }

    fn transition(
        &self,
        _state: &mut Self::State,
        _step: &EdgeStep<'_>,
        _choice: Choice,
    ) -> Result<Branch, Self::Error> {
        Ok(Branch::Keep)
    }

    fn canonicalize(&self, _state: &mut Self::State, _next: &FrontierView<'_>) {}

    fn finalize(&self, _state: &Self::State) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

#[test]
fn constant_hash_states_merge_by_eq_only_and_never_across_layers() {
    let graph = Graph::from_edges(5, [(0, 1), (1, 2), (2, 3), (3, 4)]).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    let report = FrontierBuilder::new(&space)
        .build_with_stats(LayerAgnosticProblem)
        .unwrap();

    assert_eq!(report.stats.layers_processed, graph.edge_count());
    assert_eq!(report.stats.states_merged, graph.edge_count());
    assert_eq!(family_masks(&report.value), oracle_masks(&graph, |_| true));
}

#[test]
fn periodic_five_vertex_graph_exhaustion_matches_independent_oracles() {
    if std::env::var_os("ZDD_EXTENDED_FRONTIER_CASES").is_none() {
        return;
    }
    let candidate_edges: Vec<_> = (0..5)
        .flat_map(|first| ((first + 1)..5).map(move |second| (first, second)))
        .collect();
    for graph_mask in 0..(1usize << candidate_edges.len()) {
        let edges = candidate_edges
            .iter()
            .enumerate()
            .filter_map(|(index, &edge)| (graph_mask & (1 << index) != 0).then_some(edge));
        let graph = Graph::from_edges(5, edges).unwrap();
        let space = GraphSpace::new(&graph).unwrap();

        let matching = space.matchings().unwrap();
        assert_eq!(
            family_masks(&matching),
            oracle_masks(&graph, |mask| matching_accepts(&graph, mask))
        );
        let cycles = space.cycles().unwrap();
        assert_eq!(
            family_masks(&cycles),
            oracle_masks(&graph, |mask| cycle_accepts(&graph, mask))
        );
        for source in 0..5 {
            for target in (source + 1)..5 {
                let paths = space
                    .paths(
                        graph.vertex_id(source).unwrap(),
                        graph.vertex_id(target).unwrap(),
                    )
                    .unwrap();
                assert_eq!(
                    family_masks(&paths),
                    oracle_masks(&graph, |mask| path_accepts(&graph, source, target, mask))
                );
            }
        }
    }
}
