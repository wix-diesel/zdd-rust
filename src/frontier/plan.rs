use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::ops::Range;

use crate::{EdgeId, GraphError, GraphSpace, VariableId, VertexId};

/// A compact index into the largest working frontier of a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrontierSlot(u32);

impl FrontierSlot {
    /// Returns the zero-based slot index.
    #[must_use]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug)]
struct StepData {
    edge: EdgeId,
    variable: VariableId,
    endpoints: [VertexId; 2],
    endpoint_slots: [FrontierSlot; 2],
    remaining_incident: [usize; 2],
    introduced: Range<usize>,
    forgotten: Range<usize>,
    width_before: usize,
    working_width: usize,
    width_after: usize,
}

/// An `O(n + m)` schedule for processing a graph in a fixed edge order.
///
/// The plan stores each non-isolated vertex once in the introduction events
/// and once in the forgetting events. It does not retain a frontier snapshot
/// for every edge. Slots remain stable from a vertex's introduction through
/// its forgetting, and at most [`Self::max_working_frontier_width`] slots are
/// required by a consumer.
#[derive(Clone, Debug)]
pub struct FrontierPlan {
    steps: Vec<StepData>,
    introduced: Vec<VertexId>,
    forgotten: Vec<VertexId>,
    first_incident: Vec<Option<usize>>,
    last_incident: Vec<Option<usize>>,
    vertex_slots: Vec<Option<FrontierSlot>>,
    max_frontier_width: usize,
    max_working_frontier_width: usize,
}

impl FrontierPlan {
    /// Builds a plan from the edge order fixed by `space`.
    pub fn new(space: &GraphSpace) -> Result<Self, GraphError> {
        let graph = space.graph();
        let mut endpoints = Vec::with_capacity(graph.edge_count());
        let mut first_incident = vec![None; graph.vertex_count()];
        let mut last_incident = vec![None; graph.vertex_count()];
        let mut remaining = vec![0usize; graph.vertex_count()];

        for level in 0..graph.edge_count() {
            let variable = space.as_family_space().variable(level)?;
            let edge = space.edge_for_variable(variable)?;
            let pair = graph.endpoints(edge)?;
            for vertex in [pair.0, pair.1] {
                first_incident[vertex.index()].get_or_insert(level);
                last_incident[vertex.index()] = Some(level);
                remaining[vertex.index()] += 1;
            }
            endpoints.push((variable, edge, [pair.0, pair.1]));
        }

        let mut introduced = Vec::with_capacity(graph.vertex_count());
        let mut forgotten = Vec::with_capacity(graph.vertex_count());
        let mut vertex_slots = vec![None; graph.vertex_count()];
        let mut steps = Vec::with_capacity(graph.edge_count());
        let mut free_slots = BinaryHeap::new();
        let mut next_slot = 0usize;
        let mut active = 0usize;
        let mut max_frontier_width = 0usize;
        let mut max_working_frontier_width = 0usize;

        for (level, (variable, edge, pair)) in endpoints.into_iter().enumerate() {
            let width_before = active;
            let introduced_start = introduced.len();
            let mut entering = pair;
            entering.sort_unstable();
            for vertex in entering {
                if first_incident[vertex.index()] == Some(level) {
                    let slot = free_slots.pop().map_or_else(
                        || {
                            let slot = next_slot;
                            next_slot += 1;
                            slot
                        },
                        |Reverse(slot)| slot,
                    );
                    let slot = FrontierSlot(slot as u32);
                    vertex_slots[vertex.index()] = Some(slot);
                    introduced.push(vertex);
                    active += 1;
                }
            }
            let working_width = active;
            // Every endpoint is assigned a slot at its first incident step,
            // which is no later than the current step, and the assignment is
            // retained after forgetting so EdgeStep can expose stable slots.
            let assigned_slot = |vertex: VertexId| {
                vertex_slots[vertex.index()]
                    .expect("an edge endpoint must have an assigned frontier slot")
            };

            let mut remaining_incident = [0; 2];
            for (position, vertex) in pair.into_iter().enumerate() {
                remaining[vertex.index()] -= 1;
                remaining_incident[position] = remaining[vertex.index()];
            }

            let forgotten_start = forgotten.len();
            let mut leaving = pair;
            leaving.sort_unstable();
            for vertex in leaving {
                if last_incident[vertex.index()] == Some(level) {
                    forgotten.push(vertex);
                    let slot = assigned_slot(vertex);
                    free_slots.push(Reverse(slot.index()));
                    active -= 1;
                }
            }
            let width_after = active;
            max_frontier_width = max_frontier_width.max(width_before).max(width_after);
            max_working_frontier_width = max_working_frontier_width.max(working_width);
            let endpoint_slots = [assigned_slot(pair[0]), assigned_slot(pair[1])];
            steps.push(StepData {
                edge,
                variable,
                endpoints: pair,
                endpoint_slots,
                remaining_incident,
                introduced: introduced_start..introduced.len(),
                forgotten: forgotten_start..forgotten.len(),
                width_before,
                working_width,
                width_after,
            });
        }

        Ok(Self {
            steps,
            introduced,
            forgotten,
            first_incident,
            last_incident,
            vertex_slots,
            max_frontier_width,
            max_working_frontier_width,
        })
    }

    /// Returns the number of edge-processing steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Returns whether the plan contains no edge-processing steps.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Returns the largest frontier at an edge-layer boundary.
    #[must_use]
    pub fn max_frontier_width(&self) -> usize {
        self.max_frontier_width
    }

    /// Returns the number of slots needed while processing an edge.
    #[must_use]
    pub fn max_working_frontier_width(&self) -> usize {
        self.max_working_frontier_width
    }

    /// Returns the first ordered edge position incident to `vertex`.
    pub fn first_incident_step(&self, vertex: VertexId) -> Result<Option<usize>, GraphError> {
        self.first_incident
            .get(vertex.index())
            .copied()
            .ok_or(GraphError::InvalidVertex {
                index: vertex.index(),
                vertex_count: self.first_incident.len(),
            })
    }

    /// Returns the last ordered edge position incident to `vertex`.
    pub fn last_incident_step(&self, vertex: VertexId) -> Result<Option<usize>, GraphError> {
        self.last_incident
            .get(vertex.index())
            .copied()
            .ok_or(GraphError::InvalidVertex {
                index: vertex.index(),
                vertex_count: self.last_incident.len(),
            })
    }

    /// Returns the stable working-frontier slot assigned to `vertex`.
    pub fn slot_for_vertex(&self, vertex: VertexId) -> Result<Option<FrontierSlot>, GraphError> {
        self.vertex_slots
            .get(vertex.index())
            .copied()
            .ok_or(GraphError::InvalidVertex {
                index: vertex.index(),
                vertex_count: self.vertex_slots.len(),
            })
    }

    /// Returns a borrowed view of one edge-processing step.
    #[must_use]
    pub fn step(&self, index: usize) -> Option<EdgeStep<'_>> {
        self.steps
            .get(index)
            .map(|data| EdgeStep { plan: self, data })
    }

    /// Iterates over edge-processing steps without allocating snapshots.
    #[must_use]
    pub fn steps(&self) -> impl ExactSizeIterator<Item = EdgeStep<'_>> {
        self.steps.iter().map(|data| EdgeStep { plan: self, data })
    }
}

/// A borrowed view of one edge-processing step in a [`FrontierPlan`].
#[derive(Clone, Copy, Debug)]
pub struct EdgeStep<'a> {
    plan: &'a FrontierPlan,
    data: &'a StepData,
}

impl<'a> EdgeStep<'a> {
    /// Returns the original graph edge processed at this step.
    #[must_use]
    pub fn edge(self) -> EdgeId {
        self.data.edge
    }

    /// Returns the family-space variable for this step.
    #[must_use]
    pub fn variable(self) -> VariableId {
        self.data.variable
    }

    /// Returns the edge endpoints in their input orientation.
    #[must_use]
    pub fn endpoints(self) -> [VertexId; 2] {
        self.data.endpoints
    }

    /// Returns endpoint slots in the same order as [`Self::endpoints`].
    #[must_use]
    pub fn endpoint_slots(self) -> [FrontierSlot; 2] {
        self.data.endpoint_slots
    }

    /// Returns vertices introduced before this edge is processed.
    #[must_use]
    pub fn introduced(self) -> &'a [VertexId] {
        &self.plan.introduced[self.data.introduced.clone()]
    }

    /// Returns vertices forgotten after this edge is processed.
    #[must_use]
    pub fn forgotten(self) -> &'a [VertexId] {
        &self.plan.forgotten[self.data.forgotten.clone()]
    }

    /// Returns the number of active vertices immediately before introduction.
    #[must_use]
    pub fn frontier_width_before(self) -> usize {
        self.data.width_before
    }

    /// Returns the number of active vertices after introduction and before forgetting.
    #[must_use]
    pub fn working_frontier_width(self) -> usize {
        self.data.working_width
    }

    /// Returns the number of active vertices after forgetting.
    #[must_use]
    pub fn frontier_width_after(self) -> usize {
        self.data.width_after
    }

    /// Returns remaining incident-edge counts after this edge is processed.
    ///
    /// Values correspond to [`Self::endpoints`].
    #[must_use]
    pub fn remaining_incident_edges(self) -> [usize; 2] {
        self.data.remaining_incident
    }
}
