use std::convert::Infallible;

use crate::{Branch, Choice, EdgeStep, FrontierProblem, FrontierView, Graph};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MatchingState {
    matched_vertices: Vec<bool>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MatchingProblem;

impl FrontierProblem for MatchingProblem {
    type State = MatchingState;
    type Error = Infallible;

    fn initial_state(&self, graph: &Graph) -> Result<Self::State, Self::Error> {
        Ok(MatchingState {
            matched_vertices: vec![false; graph.vertex_count()],
        })
    }

    fn transition(
        &self,
        state: &mut Self::State,
        step: &EdgeStep<'_>,
        choice: Choice,
    ) -> Result<Branch, Self::Error> {
        if choice == Choice::Include {
            let [first, second] = step.endpoints();
            if state.matched_vertices[first.index()] || state.matched_vertices[second.index()] {
                return Ok(Branch::Reject);
            }
            state.matched_vertices[first.index()] = true;
            state.matched_vertices[second.index()] = true;
        }

        for &vertex in step.forgotten() {
            state.matched_vertices[vertex.index()] = false;
        }
        Ok(Branch::Keep)
    }

    fn canonicalize(&self, _state: &mut Self::State, _next: &FrontierView<'_>) {}

    fn finalize(&self, _state: &Self::State) -> Result<bool, Self::Error> {
        Ok(true)
    }
}
