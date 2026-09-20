use std::sync::Arc;

use crate::{SetFamily, VariableId};

use super::{EdgeFamily, EdgeOrder, GraphError, GraphSpace};

impl GraphSpace {
    /// Copies several edge-family roots into a fresh graph space.
    ///
    /// The graph, fixed variable order, and original [`super::EdgeId`] mapping
    /// are preserved. Results correspond to `families` in input order and keep
    /// sharing between their reachable DAGs. Source families remain valid; old
    /// manager memory remains allocated while any old space or family is owned.
    pub fn compact(
        &self,
        families: &[EdgeFamily],
    ) -> Result<(GraphSpace, Vec<EdgeFamily>), GraphError> {
        if !families
            .iter()
            .all(|family| Arc::ptr_eq(&self.inner, &family.context))
        {
            return Err(GraphError::ContextMismatch {});
        }

        let order = EdgeOrder::new(
            &self.inner.graph,
            self.inner.variable_to_edge.iter().copied(),
        )?;
        let destination = Self::builder(&self.inner.graph)
            .ordering(order)
            .limits(self.inner.family_space.limits().clone())
            .build()?;
        let source_families = families
            .iter()
            .map(|family| family.family.clone())
            .collect::<Vec<SetFamily>>();
        let variable_map = (0..self.inner.graph.edge_count())
            .map(|index| destination.inner.family_space.variable(index))
            .collect::<Result<Vec<VariableId>, _>>()?;
        let imported = destination.inner.family_space.import_many(
            &self.inner.family_space,
            &source_families,
            &variable_map,
        )?;
        let families = imported
            .into_iter()
            .map(|family| destination.family(family))
            .collect();
        Ok((destination, families))
    }
}
