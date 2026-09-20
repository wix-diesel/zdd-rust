use super::*;

impl FamilySpace {
    /// Imports `source` through an explicit source-to-destination variable map.
    ///
    /// The map must contain one destination variable for every source variable,
    /// be a bijection, and preserve variable order. Equal universe sizes alone
    /// never imply compatible element meanings.
    pub fn import(
        &self,
        source: &SetFamily,
        variable_map: &[VariableId],
    ) -> Result<SetFamily, Error> {
        let mut imported =
            self.import_many_from(&source.space, std::slice::from_ref(source), variable_map)?;
        Ok(imported.pop().expect("one source produces one result"))
    }

    /// Copies several roots from this space into a fresh space.
    ///
    /// The returned families correspond to `families` in input order. Their
    /// reachable DAG is copied once, so sharing between roots is retained. The
    /// source space and roots are unchanged and remain valid. Consequently,
    /// retaining any old space or family also retains its old manager memory;
    /// compaction temporarily needs both the source snapshot and destination
    /// DAG, and releases old memory only after all old owners are dropped.
    pub fn compact(&self, families: &[SetFamily]) -> Result<(FamilySpace, Vec<SetFamily>), Error> {
        self.ensure_owns_all(families)?;
        let destination = Self::builder(self.inner.variable_count)
            .limits(self.inner.limits.clone())
            .build()?;
        let variable_map = (0..self.inner.variable_count)
            .map(|index| destination.variable(index))
            .collect::<Result<Vec<_>, _>>()?;
        let imported = destination.import_many_from(&self.inner, families, &variable_map)?;
        Ok((destination, imported))
    }

    fn import_many_from(
        &self,
        source_space: &Arc<SpaceInner>,
        sources: &[SetFamily],
        variable_map: &[VariableId],
    ) -> Result<Vec<SetFamily>, Error> {
        Self::validate_variable_map(
            source_space.variable_count,
            self.inner.variable_count,
            variable_map,
        )?;
        for source in sources {
            if !Arc::ptr_eq(source_space, &source.space) {
                return Err(Error::ContextMismatch {});
            }
        }
        if sources.is_empty() {
            return Ok(Vec::new());
        }

        let roots = sources
            .iter()
            .map(|source| &source.root)
            .collect::<Vec<_>>();
        let snapshot: TransferDag = source_space.manager.transfer_snapshot(&roots);
        let map = variable_map
            .iter()
            .map(|variable| variable.0)
            .collect::<Vec<_>>();
        let nodes_before = self.inner.manager.inner_node_count();
        match self.inner.manager.import_dag(&snapshot, &map) {
            Ok((roots, _)) => Ok(roots.into_iter().map(|root| self.family(root)).collect()),
            Err(error) => {
                let nodes_after = self.inner.manager.inner_node_count();
                Err(Error::LimitExceeded {
                    kind: LimitKind::Node,
                    limit: self.inner.limits.max_live_nodes,
                    attempted: self.inner.limits.max_live_nodes.saturating_add(1),
                    stats: OperationStats::node_construction(
                        nodes_before,
                        nodes_after,
                        error.nodes_created,
                    ),
                })
            }
        }
    }

    #[cfg(feature = "graph")]
    pub(crate) fn import_many(
        &self,
        source_space: &FamilySpace,
        sources: &[SetFamily],
        variable_map: &[VariableId],
    ) -> Result<Vec<SetFamily>, Error> {
        self.import_many_from(&source_space.inner, sources, variable_map)
    }

    fn ensure_owns_all(&self, families: &[SetFamily]) -> Result<(), Error> {
        if families
            .iter()
            .all(|family| Arc::ptr_eq(&self.inner, &family.space))
        {
            Ok(())
        } else {
            Err(Error::ContextMismatch {})
        }
    }

    fn validate_variable_map(
        source_count: usize,
        destination_count: usize,
        variable_map: &[VariableId],
    ) -> Result<(), Error> {
        if source_count != destination_count {
            return Err(Error::UniverseSizeMismatch {
                source: source_count,
                destination: destination_count,
            });
        }
        if variable_map.len() != source_count {
            return Err(Error::InvalidVariableMapLength {
                expected: source_count,
                actual: variable_map.len(),
            });
        }

        let mut first_sources = vec![None; destination_count];
        for (source_index, &destination) in variable_map.iter().enumerate() {
            let destination_index = destination.index();
            if destination_index >= destination_count {
                return Err(Error::InvalidMappedVariable {
                    source_index,
                    destination_index,
                    destination_variable_count: destination_count,
                });
            }
            if let Some(first_source_index) = first_sources[destination_index].replace(source_index)
            {
                return Err(Error::DuplicateMappedVariable {
                    destination_index,
                    first_source_index,
                    duplicate_source_index: source_index,
                });
            }
        }
        if variable_map
            .windows(2)
            .any(|pair| pair[0].index() >= pair[1].index())
        {
            return Err(Error::OrderMismatch {});
        }
        Ok(())
    }
}
