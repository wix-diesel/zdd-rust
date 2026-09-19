use std::hash::Hash;

use crate::Graph;

use super::{EdgeStep, FrontierView};

/// The decision made for the edge at the current frontier step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Choice {
    /// Do not select the current edge.
    Exclude,
    /// Select the current edge.
    Include,
}

/// The outcome of one user-defined state transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Branch {
    /// Retain the updated state in the next layer.
    Keep,
    /// Reject this branch as infeasible.
    Reject,
}

/// A safe, user-defined frontier dynamic-programming problem.
///
/// States are compared only within one layer. After [`Self::canonicalize`],
/// equal states must have exactly the same accepted suffix edge sets from that
/// layer. Hash collisions are permitted and are always resolved with full
/// [`Eq`] comparison. `transition`, `canonicalize`, `Hash`, and `Eq` must be
/// deterministic while a build is running.
///
/// No method runs while the destination ZDD manager is locked. A state should
/// contain problem data only; backend node identifiers are neither exposed nor
/// required.
pub trait FrontierProblem {
    /// Dynamic-programming state retained at a layer boundary.
    type State: Clone + Eq + Hash;
    /// Problem-specific error preserved by [`super::BuildError::Problem`].
    type Error;

    /// Creates and validates the state before any edge is processed.
    fn initial_state(&self, graph: &Graph) -> Result<Self::State, Self::Error>;

    /// Applies one edge choice, including introduction and forgetting work.
    ///
    /// The builder starts both choices from the same source state. A rejected
    /// mutation is discarded and cannot affect the other choice.
    fn transition(
        &self,
        state: &mut Self::State,
        step: &EdgeStep<'_>,
        choice: Choice,
    ) -> Result<Branch, Self::Error>;

    /// Converts a kept state to its deterministic layer-boundary form.
    fn canonicalize(&self, state: &mut Self::State, next: &FrontierView<'_>);

    /// Returns whether a state after the final edge is accepting.
    fn finalize(&self, state: &Self::State) -> Result<bool, Self::Error>;
}
