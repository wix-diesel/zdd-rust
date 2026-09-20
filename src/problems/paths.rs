use std::collections::HashMap;

use crate::{Branch, Choice, EdgeStep, FrontierProblem, FrontierView, Graph, GraphError, VertexId};

const SOURCE: u8 = 1;
const TARGET: u8 = 2;
const BOTH_ENDPOINTS: u8 = SOURCE | TARGET;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(super) struct PathSlot {
    pub(super) degree: u8,
    pub(super) component: Option<u32>,
    pub(super) endpoint_mask: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PathState {
    pub(super) slots: Vec<PathSlot>,
    pub(super) done: bool,
    pub(super) possible: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PathProblem {
    source: VertexId,
    target: VertexId,
}

impl PathProblem {
    pub(crate) fn new(source: VertexId, target: VertexId) -> Self {
        Self { source, target }
    }

    fn endpoint_mask(self, vertex: VertexId) -> u8 {
        (u8::from(vertex == self.source) * SOURCE) | (u8::from(vertex == self.target) * TARGET)
    }

    fn maximum_degree(self, vertex: VertexId) -> u8 {
        if vertex == self.source || vertex == self.target {
            1
        } else {
            2
        }
    }

    fn introduce(self, state: &mut PathState, step: &EdgeStep<'_>) {
        let slots = step.endpoint_slots();
        let required_len = slots.iter().map(|slot| slot.index() + 1).max().unwrap_or(0);
        if state.slots.len() < required_len {
            state.slots.resize(required_len, PathSlot::default());
        }
        for (vertex, slot) in step.endpoints().into_iter().zip(slots) {
            if step.introduced().contains(&vertex) {
                state.slots[slot.index()] = PathSlot::default();
            }
        }
    }

    fn include(self, state: &mut PathState, step: &EdgeStep<'_>) -> Branch {
        if state.done {
            return Branch::Reject;
        }
        let vertices = step.endpoints();
        let slots = step.endpoint_slots();
        if vertices
            .into_iter()
            .zip(slots)
            .any(|(vertex, slot)| state.slots[slot.index()].degree >= self.maximum_degree(vertex))
        {
            return Branch::Reject;
        }

        let first = state.slots[slots[0].index()];
        let second = state.slots[slots[1].index()];
        if first.component.is_some() && first.component == second.component {
            return Branch::Reject;
        }
        let component = first
            .component
            .or(second.component)
            .unwrap_or_else(|| self.next_component(state));
        let mask = first.endpoint_mask
            | second.endpoint_mask
            | self.endpoint_mask(vertices[0])
            | self.endpoint_mask(vertices[1]);

        if let Some(old) = second.component.filter(|&old| old != component) {
            for slot in &mut state.slots {
                if slot.component == Some(old) {
                    slot.component = Some(component);
                    slot.endpoint_mask = mask;
                }
            }
        }
        for slot in &mut state.slots {
            if slot.component == Some(component) {
                slot.endpoint_mask = mask;
            }
        }
        for slot in slots {
            let entry = &mut state.slots[slot.index()];
            entry.degree += 1;
            entry.component = Some(component);
            entry.endpoint_mask = mask;
        }
        Branch::Keep
    }

    fn next_component(self, state: &PathState) -> u32 {
        state
            .slots
            .iter()
            .filter_map(|slot| slot.component)
            .max()
            .map_or(0, |label| label + 1)
    }

    fn forget(self, state: &mut PathState, step: &EdgeStep<'_>) -> Branch {
        let mut closing = Vec::new();
        for &vertex in step.forgotten() {
            let slot = step
                .endpoints()
                .into_iter()
                .zip(step.endpoint_slots())
                .find_map(|(endpoint, slot)| (endpoint == vertex).then_some(slot))
                .expect("a forgotten vertex at a step is one of that edge's endpoints");
            let entry = state.slots[slot.index()];
            let valid_degree = if vertex == self.source || vertex == self.target {
                entry.degree == 1
            } else {
                entry.degree == 0 || entry.degree == 2
            };
            if !valid_degree {
                return Branch::Reject;
            }
            if let Some(component) = entry.component {
                closing.push((component, entry.endpoint_mask));
            }
            state.slots[slot.index()] = PathSlot::default();
        }

        closing.sort_unstable();
        closing.dedup();
        for (component, mask) in closing {
            let remains_open = state
                .slots
                .iter()
                .any(|slot| slot.component == Some(component));
            if remains_open {
                continue;
            }
            let another_component = state.slots.iter().any(|slot| slot.component.is_some());
            if mask != BOTH_ENDPOINTS || another_component || state.done {
                return Branch::Reject;
            }
            state.done = true;
        }
        Branch::Keep
    }
}

impl FrontierProblem for PathProblem {
    type State = PathState;
    type Error = GraphError;

    fn initial_state(&self, graph: &Graph) -> Result<Self::State, Self::Error> {
        graph.vertex_id(self.source.index())?;
        graph.vertex_id(self.target.index())?;
        if self.source == self.target {
            return Err(GraphError::IdenticalPathEndpoints {
                vertex: self.source.index(),
            });
        }
        let mut incident = [false; 2];
        for index in 0..graph.edge_count() {
            let endpoints = graph.endpoints(graph.edge_id(index)?)?;
            incident[0] |= endpoints.0 == self.source || endpoints.1 == self.source;
            incident[1] |= endpoints.0 == self.target || endpoints.1 == self.target;
        }
        Ok(PathState {
            slots: Vec::new(),
            done: false,
            possible: incident[0] && incident[1],
        })
    }

    fn transition(
        &self,
        state: &mut Self::State,
        step: &EdgeStep<'_>,
        choice: Choice,
    ) -> Result<Branch, Self::Error> {
        if !state.possible {
            return Ok(Branch::Reject);
        }
        self.introduce(state, step);
        if choice == Choice::Include && self.include(state, step) == Branch::Reject {
            return Ok(Branch::Reject);
        }
        Ok(self.forget(state, step))
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
        Ok(state.possible && state.done)
    }
}
