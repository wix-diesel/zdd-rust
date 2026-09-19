use std::collections::HashMap;
use std::convert::Infallible;

use crate::{Branch, Choice, EdgeStep, FrontierProblem, FrontierView, Graph};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
struct CycleSlot {
    degree: u8,
    component: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CycleState {
    slots: Vec<CycleSlot>,
    done: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CycleProblem;

impl CycleProblem {
    fn introduce(state: &mut CycleState, step: &EdgeStep<'_>) {
        let slots = step.endpoint_slots();
        let required_len = slots.iter().map(|slot| slot.index() + 1).max().unwrap_or(0);
        if state.slots.len() < required_len {
            state.slots.resize(required_len, CycleSlot::default());
        }
        for (vertex, slot) in step.endpoints().into_iter().zip(slots) {
            if step.introduced().contains(&vertex) {
                state.slots[slot.index()] = CycleSlot::default();
            }
        }
    }

    fn include(state: &mut CycleState, step: &EdgeStep<'_>) -> Branch {
        if state.done {
            return Branch::Reject;
        }
        let slots = step.endpoint_slots();
        let first = state.slots[slots[0].index()];
        let second = state.slots[slots[1].index()];
        if first.degree >= 2 || second.degree >= 2 {
            return Branch::Reject;
        }

        if let Some(component) = first
            .component
            .filter(|&component| second.component == Some(component))
        {
            if state
                .slots
                .iter()
                .any(|slot| slot.component.is_some_and(|other| other != component))
            {
                return Branch::Reject;
            }
            state.slots[slots[0].index()].degree += 1;
            state.slots[slots[1].index()].degree += 1;
            state.done = true;
            return Branch::Keep;
        }

        let component = first
            .component
            .or(second.component)
            .unwrap_or_else(|| Self::next_component(state));
        if let Some(old) = second.component.filter(|&old| old != component) {
            for slot in &mut state.slots {
                if slot.component == Some(old) {
                    slot.component = Some(component);
                }
            }
        }
        for slot in slots {
            let entry = &mut state.slots[slot.index()];
            entry.degree += 1;
            entry.component = Some(component);
        }
        Branch::Keep
    }

    fn next_component(state: &CycleState) -> u32 {
        state
            .slots
            .iter()
            .filter_map(|slot| slot.component)
            .max()
            .map_or(0, |label| label + 1)
    }

    fn forget(state: &mut CycleState, step: &EdgeStep<'_>) -> Branch {
        let mut disappearing = Vec::new();
        for &vertex in step.forgotten() {
            let slot = step
                .endpoints()
                .into_iter()
                .zip(step.endpoint_slots())
                .find_map(|(endpoint, slot)| (endpoint == vertex).then_some(slot))
                .expect("a forgotten vertex at a step is one of that edge's endpoints");
            let entry = state.slots[slot.index()];
            if entry.degree != 0 && entry.degree != 2 {
                return Branch::Reject;
            }
            if let Some(component) = entry.component {
                disappearing.push(component);
            }
            state.slots[slot.index()] = CycleSlot::default();
        }

        disappearing.sort_unstable();
        disappearing.dedup();
        for component in disappearing {
            let remains_open = state
                .slots
                .iter()
                .any(|slot| slot.component == Some(component));
            if !remains_open && !state.done {
                return Branch::Reject;
            }
        }
        Branch::Keep
    }
}

impl FrontierProblem for CycleProblem {
    type State = CycleState;
    type Error = Infallible;

    fn initial_state(&self, _graph: &Graph) -> Result<Self::State, Self::Error> {
        Ok(CycleState {
            slots: Vec::new(),
            done: false,
        })
    }

    fn transition(
        &self,
        state: &mut Self::State,
        step: &EdgeStep<'_>,
        choice: Choice,
    ) -> Result<Branch, Self::Error> {
        Self::introduce(state, step);
        if choice == Choice::Include && Self::include(state, step) == Branch::Reject {
            return Ok(Branch::Reject);
        }
        Ok(Self::forget(state, step))
    }

    fn canonicalize(&self, state: &mut Self::State, next: &FrontierView<'_>) {
        let active_len = next
            .iter()
            .map(|(slot, _)| slot.index() + 1)
            .max()
            .unwrap_or(0);
        state.slots.truncate(active_len);

        let mut labels = HashMap::new();
        let mut next_label = 0u32;
        for slot in &mut state.slots {
            if let Some(old) = slot.component {
                let label = *labels.entry(old).or_insert_with(|| {
                    let label = next_label;
                    next_label += 1;
                    label
                });
                slot.component = Some(label);
            }
        }
    }

    fn finalize(&self, state: &Self::State) -> Result<bool, Self::Error> {
        Ok(state.done)
    }
}
