use super::*;

impl CountIndex {
    pub(super) fn from_dag(
        dag: QueryDag,
        limits: Option<&QueryLimits>,
    ) -> Result<Self, QueryError> {
        let mut counts = vec![None; dag.nodes.len() + 2];
        let mut stats = QueryStats {
            snapshot_nodes: dag.nodes.len(),
            ..QueryStats::default()
        };
        let mut work = vec![CountWork::Visit(dag.root)];

        while let Some(item) = work.pop() {
            match item {
                CountWork::Visit(reference) => {
                    if counts[reference].is_some() {
                        continue;
                    }
                    match reference {
                        QUERY_ZERO => Self::store_count(
                            reference,
                            BigUint::from(0u8),
                            &mut counts,
                            &mut stats,
                            limits,
                        )?,
                        QUERY_ONE => Self::store_count(
                            reference,
                            BigUint::from(1u8),
                            &mut counts,
                            &mut stats,
                            limits,
                        )?,
                        _ => {
                            let node = &dag.nodes[reference - 2];
                            for child in [node.hi, node.lo] {
                                if child >= 2 {
                                    debug_assert!(
                                        node.variable < dag.nodes[child - 2].variable,
                                        "ZDD children must follow their parent variable"
                                    );
                                }
                            }
                            work.push(CountWork::Finish(reference));
                            work.push(CountWork::Visit(node.hi));
                            work.push(CountWork::Visit(node.lo));
                        }
                    }
                }
                CountWork::Finish(reference) => {
                    if counts[reference].is_some() {
                        continue;
                    }
                    let node = &dag.nodes[reference - 2];
                    let value = counts[node.lo]
                        .as_ref()
                        .expect("LO count is computed before its parent")
                        + counts[node.hi]
                            .as_ref()
                            .expect("HI count is computed before its parent");
                    Self::store_count(reference, value, &mut counts, &mut stats, limits)?;
                }
            }
        }

        Ok(Self { dag, counts, stats })
    }

    fn store_count(
        reference: usize,
        value: BigUint,
        counts: &mut [Option<BigUint>],
        stats: &mut QueryStats,
        limits: Option<&QueryLimits>,
    ) -> Result<(), QueryError> {
        let bits =
            usize::try_from(value.bits()).expect("64-bit targets represent BigUint bit sizes");
        let attempted = stats.total_count_bits.saturating_add(bits);
        if let Some(limits) = limits
            && attempted > limits.max_total_count_bits
        {
            return Err(QueryError::LimitExceeded {
                kind: LimitKind::CountBits,
                limit: limits.max_total_count_bits,
                attempted,
                stats: stats.clone(),
            });
        }
        stats.total_count_bits = attempted;
        stats.max_count_bits = stats.max_count_bits.max(bits);
        counts[reference] = Some(value);
        Ok(())
    }

    /// Returns the exact number of sets represented by this index.
    #[must_use]
    pub fn count(&self) -> &BigUint {
        self.counts[self.dag.root]
            .as_ref()
            .expect("the root count is always retained")
    }

    /// Returns the resource measurements from index construction.
    #[must_use]
    pub fn stats(&self) -> &QueryStats {
        &self.stats
    }
}
