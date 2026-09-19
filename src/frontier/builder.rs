use std::collections::HashMap;

use crate::{
    CancellationToken, EdgeFamily, GraphSpace, LimitKind, Limits, SetFamily, VariableId, VertexId,
};

use super::{
    Branch, BuildError, BuildReport, BuildStats, Choice, FrontierPlan, FrontierProblem,
    FrontierView,
};

#[derive(Clone, Copy, Debug)]
enum Target {
    Reject,
    State(usize),
}

#[derive(Debug)]
struct TapeLayer {
    variable: VariableId,
    transitions: Vec<[Target; 2]>,
}

/// Configures and runs a layered frontier dynamic program in a graph space.
///
/// The forward phase retains user states for only the current and next layer,
/// while its compact transition tape necessarily spans every edge layer. The
/// backward phase then converts that tape to a canonical ZDD.
pub struct FrontierBuilder<'a> {
    space: &'a GraphSpace,
    max_frontier_states: usize,
    max_frontier_transitions: usize,
    cancellation: Option<CancellationToken>,
}

impl<'a> FrontierBuilder<'a> {
    /// Creates a builder using the frontier limits fixed by `space`.
    #[must_use]
    pub fn new(space: &'a GraphSpace) -> Self {
        let limits = space.as_family_space().limits();
        Self {
            space,
            max_frontier_states: limits.max_frontier_states,
            max_frontier_transitions: limits.max_frontier_transitions,
            cancellation: None,
        }
    }

    /// Applies per-build frontier limits, capped by the space-wide limits.
    ///
    /// Only `max_frontier_states` and `max_frontier_transitions` are used; ZDD
    /// node capacity remains the fixed manager-wide limit of the graph space.
    #[must_use]
    pub fn limits(mut self, limits: Limits) -> Self {
        let space_limits = self.space.as_family_space().limits();
        self.max_frontier_states = limits
            .max_frontier_states
            .min(space_limits.max_frontier_states);
        self.max_frontier_transitions = limits
            .max_frontier_transitions
            .min(space_limits.max_frontier_transitions);
        self
    }

    /// Enables cooperative cancellation with a shared token.
    #[must_use]
    pub fn cancellation_token(mut self, token: CancellationToken) -> Self {
        self.cancellation = Some(token);
        self
    }

    /// Builds the complete accepted edge family.
    pub fn build<P: FrontierProblem>(self, problem: P) -> Result<EdgeFamily, BuildError<P::Error>> {
        Ok(self.build_with_stats(problem)?.value)
    }

    /// Builds the complete accepted edge family and returns diagnostic statistics.
    pub fn build_with_stats<P: FrontierProblem>(
        self,
        problem: P,
    ) -> Result<BuildReport<EdgeFamily>, BuildError<P::Error>> {
        let mut stats = BuildStats::default();
        stats.operation.nodes_before = self.space.stats().live_nodes;
        let plan = FrontierPlan::new(self.space).map_err(|source| {
            self.finish_stats(&mut stats);
            BuildError::Graph {
                source,
                stats: Box::new(stats.clone()),
            }
        })?;

        self.check_cancelled(&mut stats)?;
        let mut active_slots = vec![None; plan.max_working_frontier_width()];
        let initial_view = FrontierView::new(&active_slots, 0);
        let mut initial = problem
            .initial_state(self.space.graph())
            .map_err(|source| self.problem_error(source, &mut stats))?;
        problem.canonicalize(&mut initial, &initial_view);
        if self.max_frontier_states == 0 {
            return Err(self.limit_error(LimitKind::FrontierStates, 0, 1, &mut stats));
        }
        stats.current_frontier_states = 1;
        stats.peak_frontier_states = 1;

        let mut states = vec![initial];
        let mut tape = Vec::with_capacity(plan.len());
        for step in plan.steps() {
            Self::advance_frontier(
                &plan,
                step.introduced(),
                step.forgotten(),
                &mut active_slots,
            );
            let next_view = FrontierView::new(&active_slots, step.frontier_width_after());
            let mut next_ids = HashMap::new();
            let mut layer = Vec::with_capacity(states.len());

            for source in &states {
                let mut reusable = None;
                let mut targets = [Target::Reject; 2];
                for (branch_index, choice) in
                    [Choice::Exclude, Choice::Include].into_iter().enumerate()
                {
                    self.check_cancelled(&mut stats)?;
                    if stats.transitions_attempted == self.max_frontier_transitions {
                        return Err(self.limit_error(
                            LimitKind::FrontierTransitions,
                            self.max_frontier_transitions,
                            stats.transitions_attempted.saturating_add(1),
                            &mut stats,
                        ));
                    }
                    stats.transitions_attempted += 1;

                    let mut candidate = reusable.take().map_or_else(
                        || source.clone(),
                        |mut buffer: P::State| {
                            buffer.clone_from(source);
                            buffer
                        },
                    );
                    let branch = problem
                        .transition(&mut candidate, &step, choice)
                        .map_err(|source| self.problem_error(source, &mut stats))?;
                    stats.transition_tape_entries += 1;
                    if branch == Branch::Reject {
                        stats.branches_rejected += 1;
                        reusable = Some(candidate);
                        continue;
                    }

                    problem.canonicalize(&mut candidate, &next_view);
                    if let Some(&id) = next_ids.get(&candidate) {
                        stats.states_merged += 1;
                        targets[branch_index] = Target::State(id);
                        reusable = Some(candidate);
                        continue;
                    }
                    if next_ids.len() == self.max_frontier_states {
                        return Err(self.limit_error(
                            LimitKind::FrontierStates,
                            self.max_frontier_states,
                            next_ids.len().saturating_add(1),
                            &mut stats,
                        ));
                    }
                    let id = next_ids.len();
                    next_ids.insert(candidate, id);
                    targets[branch_index] = Target::State(id);
                }
                layer.push(targets);
            }

            states = Self::states_in_id_order(next_ids);
            stats.layers_processed += 1;
            stats.current_frontier_states = states.len();
            stats.peak_frontier_states = stats.peak_frontier_states.max(states.len());
            tape.push(TapeLayer {
                variable: step.variable(),
                transitions: layer,
            });
        }

        let mut roots = Vec::with_capacity(states.len());
        for state in &states {
            self.check_cancelled(&mut stats)?;
            let accepting = problem
                .finalize(state)
                .map_err(|source| self.problem_error(source, &mut stats))?;
            roots.push(if accepting {
                self.space.as_family_space().unit()
            } else {
                self.space.as_family_space().empty()
            });
        }

        let zero = self.space.as_family_space().empty();
        for layer in tape.iter().rev() {
            let mut previous = Vec::with_capacity(layer.transitions.len());
            for &[exclude, include] in &layer.transitions {
                self.check_cancelled(&mut stats)?;
                let lo = Self::target_root(exclude, &roots, &zero);
                let hi = Self::target_root(include, &roots, &zero);
                let (root, created) = self
                    .space
                    .as_family_space()
                    .make_decision_node(layer.variable, hi, lo)
                    .map_err(|()| {
                        let attempted = self.space.stats().live_nodes.saturating_add(1);
                        self.limit_error(
                            LimitKind::Node,
                            self.space.as_family_space().limits().max_live_nodes,
                            attempted,
                            &mut stats,
                        )
                    })?;
                stats.operation.nodes_created += usize::from(created);
                previous.push(root);
            }
            roots = previous;
        }

        let root = roots
            .pop()
            .expect("the initial frontier layer always contains exactly one state");
        self.finish_stats(&mut stats);
        Ok(BuildReport {
            value: self.space.family(root),
            stats,
        })
    }

    fn advance_frontier(
        plan: &FrontierPlan,
        introduced: &[VertexId],
        forgotten: &[VertexId],
        slots: &mut [Option<VertexId>],
    ) {
        for &vertex in introduced {
            let slot = plan
                .slot_for_vertex(vertex)
                .expect("plan vertices belong to its graph")
                .expect("introduced vertices always have a slot");
            slots[slot.index()] = Some(vertex);
        }
        for &vertex in forgotten {
            let slot = plan
                .slot_for_vertex(vertex)
                .expect("plan vertices belong to its graph")
                .expect("forgotten vertices always have a slot");
            slots[slot.index()] = None;
        }
    }

    fn states_in_id_order<S>(states: HashMap<S, usize>) -> Vec<S> {
        let mut ordered: Vec<Option<S>> = (0..states.len()).map(|_| None).collect();
        for (state, id) in states {
            ordered[id] = Some(state);
        }
        ordered
            .into_iter()
            .map(|state| state.expect("state IDs are contiguous"))
            .collect()
    }

    fn target_root<'b>(
        target: Target,
        roots: &'b [SetFamily],
        zero: &'b SetFamily,
    ) -> &'b SetFamily {
        match target {
            Target::Reject => zero,
            Target::State(id) => &roots[id],
        }
    }

    fn check_cancelled<E>(&self, stats: &mut BuildStats) -> Result<(), BuildError<E>> {
        let Some(token) = &self.cancellation else {
            return Ok(());
        };
        stats.operation.cancellation_checks += 1;
        if token.is_cancelled() {
            self.finish_stats(stats);
            Err(BuildError::Cancelled {
                stats: Box::new(stats.clone()),
            })
        } else {
            Ok(())
        }
    }

    fn problem_error<E>(&self, source: E, stats: &mut BuildStats) -> BuildError<E> {
        self.finish_stats(stats);
        BuildError::Problem {
            source,
            stats: Box::new(stats.clone()),
        }
    }

    fn limit_error<E>(
        &self,
        kind: LimitKind,
        limit: usize,
        attempted: usize,
        stats: &mut BuildStats,
    ) -> BuildError<E> {
        self.finish_stats(stats);
        BuildError::LimitExceeded {
            kind,
            limit,
            attempted,
            stats: Box::new(stats.clone()),
        }
    }

    fn finish_stats(&self, stats: &mut BuildStats) {
        stats.operation.nodes_after = self.space.stats().live_nodes;
    }
}
