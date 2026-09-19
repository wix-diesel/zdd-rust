#![cfg(feature = "graph")]

use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use zdd_family::{
    Branch, BuildError, CancellationToken, Choice, EdgeId, EdgeStep, FrontierBuilder,
    FrontierProblem, FrontierView, Graph, GraphSpace, LimitKind, Limits,
};

#[derive(Debug)]
struct CountState {
    selected: usize,
    canonical_noise: usize,
    clone_from_calls: Arc<AtomicUsize>,
}

impl Clone for CountState {
    fn clone(&self) -> Self {
        Self {
            selected: self.selected,
            canonical_noise: self.canonical_noise,
            clone_from_calls: Arc::clone(&self.clone_from_calls),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.clone_from_calls.fetch_add(1, Ordering::Relaxed);
        self.selected = source.selected;
        self.canonical_noise = source.canonical_noise;
        self.clone_from_calls = Arc::clone(&source.clone_from_calls);
    }
}

impl PartialEq for CountState {
    fn eq(&self, other: &Self) -> bool {
        self.selected == other.selected && self.canonical_noise == other.canonical_noise
    }
}

impl Eq for CountState {}

impl Hash for CountState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Deliberately collide every state. HashMap must still use full Eq.
        0usize.hash(state);
    }
}

struct Exactly {
    selected: usize,
    clone_from_calls: Arc<AtomicUsize>,
}

impl FrontierProblem for Exactly {
    type State = CountState;
    type Error = &'static str;

    fn initial_state(&self, _graph: &Graph) -> Result<Self::State, Self::Error> {
        Ok(CountState {
            selected: 0,
            canonical_noise: 99,
            clone_from_calls: Arc::clone(&self.clone_from_calls),
        })
    }

    fn transition(
        &self,
        state: &mut Self::State,
        _step: &EdgeStep<'_>,
        choice: Choice,
    ) -> Result<Branch, Self::Error> {
        state.canonical_noise += 1;
        if choice == Choice::Include {
            state.selected += 1;
        }
        Ok(if state.selected > self.selected {
            Branch::Reject
        } else {
            Branch::Keep
        })
    }

    fn canonicalize(&self, state: &mut Self::State, _next: &FrontierView<'_>) {
        state.canonical_noise = 0;
    }

    fn finalize(&self, state: &Self::State) -> Result<bool, Self::Error> {
        Ok(state.selected == self.selected)
    }
}

fn edge_sets(family: &zdd_family::EdgeFamily) -> Vec<Vec<usize>> {
    family
        .iter()
        .map(|solution| solution.iter().map(|edge| edge.index()).collect())
        .collect()
}

#[test]
fn external_problem_merges_canonical_states_despite_hash_collisions() {
    let graph = Graph::from_edges(5, [(0, 1), (1, 2), (2, 3), (3, 4)]).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    let clone_from_calls = Arc::new(AtomicUsize::new(0));
    let report = FrontierBuilder::new(&space)
        .build_with_stats(Exactly {
            selected: 2,
            clone_from_calls: Arc::clone(&clone_from_calls),
        })
        .unwrap();

    assert_eq!(
        edge_sets(&report.value),
        vec![
            vec![2, 3],
            vec![1, 3],
            vec![1, 2],
            vec![0, 3],
            vec![0, 2],
            vec![0, 1]
        ]
    );
    assert!(report.stats.states_merged > 0);
    assert_eq!(report.stats.transition_tape_entries, 18);
    assert!(clone_from_calls.load(Ordering::Relaxed) > 0);
}

#[test]
fn zero_edge_graph_is_decided_only_by_finalize() {
    let graph = Graph::from_edges(3, []).unwrap();
    let space = GraphSpace::new(&graph).unwrap();

    let accepted = FrontierBuilder::new(&space)
        .build(Exactly {
            selected: 0,
            clone_from_calls: Arc::new(AtomicUsize::new(0)),
        })
        .unwrap();
    let rejected = FrontierBuilder::new(&space)
        .build(Exactly {
            selected: 1,
            clone_from_calls: Arc::new(AtomicUsize::new(0)),
        })
        .unwrap();

    assert_eq!(edge_sets(&accepted), vec![Vec::<usize>::new()]);
    assert!(rejected.is_empty());
}

#[test]
fn rejected_branches_are_not_added_to_the_family() {
    let graph = Graph::from_edges(2, [(0, 1)]).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    let report = FrontierBuilder::new(&space)
        .build_with_stats(Exactly {
            selected: 0,
            clone_from_calls: Arc::new(AtomicUsize::new(0)),
        })
        .unwrap();

    assert_eq!(edge_sets(&report.value), vec![Vec::<usize>::new()]);
    assert_eq!(report.stats.branches_rejected, 1);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum FailurePoint {
    Initial,
    Transition,
    Finalize,
}

struct FailingProblem(FailurePoint);

impl FrontierProblem for FailingProblem {
    type State = ();
    type Error = FailurePoint;

    fn initial_state(&self, _graph: &Graph) -> Result<Self::State, Self::Error> {
        (self.0 != FailurePoint::Initial)
            .then_some(())
            .ok_or(self.0)
    }

    fn transition(
        &self,
        _state: &mut Self::State,
        _step: &EdgeStep<'_>,
        _choice: Choice,
    ) -> Result<Branch, Self::Error> {
        if self.0 == FailurePoint::Transition {
            Err(self.0)
        } else {
            Ok(Branch::Keep)
        }
    }

    fn canonicalize(&self, _state: &mut Self::State, _next: &FrontierView<'_>) {}

    fn finalize(&self, _state: &Self::State) -> Result<bool, Self::Error> {
        if self.0 == FailurePoint::Finalize {
            Err(self.0)
        } else {
            Ok(true)
        }
    }
}

#[test]
fn problem_errors_preserve_the_original_value_and_partial_stats() {
    let graph = Graph::from_edges(2, [(0, 1)]).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    for point in [
        FailurePoint::Initial,
        FailurePoint::Transition,
        FailurePoint::Finalize,
    ] {
        let error = match FrontierBuilder::new(&space).build(FailingProblem(point)) {
            Ok(_) => panic!("expected problem error"),
            Err(error) => error,
        };
        let BuildError::Problem { source, stats, .. } = error else {
            panic!("expected problem error");
        };
        assert_eq!(source, point);
        assert!(stats.operation.nodes_after >= stats.operation.nodes_before);
    }
}

struct CancellingProblem(CancellationToken);

impl FrontierProblem for CancellingProblem {
    type State = ();
    type Error = ();

    fn initial_state(&self, _graph: &Graph) -> Result<Self::State, Self::Error> {
        Ok(())
    }

    fn transition(
        &self,
        _state: &mut Self::State,
        _step: &EdgeStep<'_>,
        _choice: Choice,
    ) -> Result<Branch, Self::Error> {
        self.0.cancel();
        Ok(Branch::Keep)
    }

    fn canonicalize(&self, _state: &mut Self::State, _next: &FrontierView<'_>) {}

    fn finalize(&self, _state: &Self::State) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

#[test]
fn cancellation_stops_between_transition_callbacks() {
    let graph = Graph::from_edges(2, [(0, 1)]).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    let token = CancellationToken::new();
    let error = match FrontierBuilder::new(&space)
        .cancellation_token(token.clone())
        .build(CancellingProblem(token))
    {
        Ok(_) => panic!("expected cancellation"),
        Err(error) => error,
    };

    let BuildError::Cancelled { stats, .. } = error else {
        panic!("expected cancellation");
    };
    assert_eq!(stats.transitions_attempted, 1);
    assert_eq!(stats.transition_tape_entries, 1);
    assert!(stats.operation.cancellation_checks >= 3);
}

fn one_edge_space(limits: Limits) -> GraphSpace {
    let graph = Graph::from_edges(2, [(0, 1)]).unwrap();
    GraphSpace::builder(&graph).limits(limits).build().unwrap()
}

#[test]
fn state_and_transition_limits_report_exact_attempts() {
    let state_space = one_edge_space(Limits::default());
    let state_error = match FrontierBuilder::new(&state_space)
        .limits(Limits {
            max_frontier_states: 1,
            ..Limits::default()
        })
        .build(Exactly {
            selected: 1,
            clone_from_calls: Arc::new(AtomicUsize::new(0)),
        }) {
        Ok(_) => panic!("expected state limit"),
        Err(error) => error,
    };
    assert!(matches!(
        state_error,
        BuildError::LimitExceeded {
            kind: LimitKind::FrontierStates,
            limit: 1,
            attempted: 2,
            ..
        }
    ));

    let transition_error = match FrontierBuilder::new(&state_space)
        .limits(Limits {
            max_frontier_transitions: 1,
            ..Limits::default()
        })
        .build(Exactly {
            selected: 1,
            clone_from_calls: Arc::new(AtomicUsize::new(0)),
        }) {
        Ok(_) => panic!("expected transition limit"),
        Err(error) => error,
    };
    let BuildError::LimitExceeded {
        kind,
        limit,
        attempted,
        stats,
        ..
    } = transition_error
    else {
        panic!("expected transition limit");
    };
    assert_eq!(kind, LimitKind::FrontierTransitions);
    assert_eq!((limit, attempted), (1, 2));
    assert_eq!(stats.transitions_attempted, 1);
    assert_eq!(stats.transition_tape_entries, 1);
}

#[test]
fn node_limit_failure_keeps_existing_families_valid() {
    let graph = Graph::from_edges(3, [(0, 1), (1, 2)]).unwrap();
    let space = GraphSpace::builder(&graph)
        .limits(Limits {
            max_live_nodes: 4,
            ..Limits::default()
        })
        .build()
        .unwrap();
    let existing = space.unit();
    let error = match FrontierBuilder::new(&space).build(Exactly {
        selected: 1,
        clone_from_calls: Arc::new(AtomicUsize::new(0)),
    }) {
        Ok(_) => panic!("expected node limit"),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        BuildError::LimitExceeded {
            kind: LimitKind::Node,
            limit: 4,
            ..
        }
    ));
    assert_eq!(edge_sets(&existing), vec![Vec::<usize>::new()]);
}

#[derive(Clone)]
struct ReentrantState {
    selected: usize,
    space: GraphSpace,
}

impl PartialEq for ReentrantState {
    fn eq(&self, other: &Self) -> bool {
        let _ = self.space.stats();
        self.selected == other.selected
    }
}

impl Eq for ReentrantState {}

impl Hash for ReentrantState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let _ = self.space.stats();
        self.selected.hash(state);
    }
}

struct ReentrantProblem(GraphSpace);

impl FrontierProblem for ReentrantProblem {
    type State = ReentrantState;
    type Error = ();

    fn initial_state(&self, _graph: &Graph) -> Result<Self::State, Self::Error> {
        let _ = self.0.unit().count();
        Ok(ReentrantState {
            selected: 0,
            space: self.0.clone(),
        })
    }

    fn transition(
        &self,
        state: &mut Self::State,
        _step: &EdgeStep<'_>,
        choice: Choice,
    ) -> Result<Branch, Self::Error> {
        let _ = self.0.stats();
        state.selected += usize::from(choice == Choice::Include);
        Ok(Branch::Keep)
    }

    fn canonicalize(&self, _state: &mut Self::State, next: &FrontierView<'_>) {
        let _ = self.0.stats();
        assert!(next.len() <= 1);
    }

    fn finalize(&self, state: &Self::State) -> Result<bool, Self::Error> {
        let edge: EdgeId = self.0.graph().edge_id(0).unwrap();
        assert!(self.0.powerset().unwrap().contains(&[edge]).unwrap());
        Ok(state.selected == 1)
    }
}

#[test]
fn user_code_and_state_hash_eq_run_without_a_manager_guard() {
    let graph = Graph::from_edges(3, [(0, 1), (1, 2)]).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    let family = FrontierBuilder::new(&space)
        .build(ReentrantProblem(space.clone()))
        .unwrap();
    assert_eq!(family.count(), 2u32.into());
}
