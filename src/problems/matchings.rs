use std::convert::Infallible;

use crate::{Branch, Choice, EdgeStep, FrontierProblem, FrontierView, Graph};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MatchingState {
    matched_slots: Vec<bool>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MatchingProblem;

impl FrontierProblem for MatchingProblem {
    type State = MatchingState;
    type Error = Infallible;

    fn initial_state(&self, _graph: &Graph) -> Result<Self::State, Self::Error> {
        Ok(MatchingState {
            matched_slots: Vec::new(),
        })
    }

    fn transition(
        &self,
        state: &mut Self::State,
        step: &EdgeStep<'_>,
        choice: Choice,
    ) -> Result<Branch, Self::Error> {
        let slots = step.endpoint_slots();
        let required_len = slots.iter().map(|slot| slot.index() + 1).max().unwrap_or(0);
        if state.matched_slots.len() < required_len {
            state.matched_slots.resize(required_len, false);
        }

        if choice == Choice::Include {
            if state.matched_slots[slots[0].index()] || state.matched_slots[slots[1].index()] {
                return Ok(Branch::Reject);
            }
            state.matched_slots[slots[0].index()] = true;
            state.matched_slots[slots[1].index()] = true;
        }

        for (slot, remaining) in slots.into_iter().zip(step.remaining_incident_edges()) {
            if remaining == 0 {
                state.matched_slots[slot.index()] = false;
            }
        }
        Ok(Branch::Keep)
    }

    fn canonicalize(&self, state: &mut Self::State, next: &FrontierView<'_>) {
        let active_len = next
            .iter()
            .map(|(slot, _vertex)| slot.index() + 1)
            .max()
            .unwrap_or(0);
        state.matched_slots.truncate(active_len);
    }

    fn finalize(&self, _state: &Self::State) -> Result<bool, Self::Error> {
        Ok(true)
    }
}
