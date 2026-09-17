use super::*;

impl ZddManager {
    // OxiDD function handles contain the manager's synchronization primitive,
    // but their Eq/Hash implementations use stable edge identities. Keeping
    // cloned roots in both tables also prevents their nodes from being freed.
    #[allow(clippy::mutable_key_type)]
    pub(crate) fn apply(
        &self,
        op: ApplyOp,
        left: &Root,
        right: &Root,
        memo_limit: usize,
    ) -> Result<(Root, ApplyStats), ApplyError> {
        let root_key = Self::key(op, left.clone(), right.clone());
        let zero = self.empty();
        let mut stats = ApplyStats::default();
        let mut memo_keys = HashSet::new();
        let mut results = HashMap::<ApplyKey, Root>::new();
        let mut work = vec![Work::Visit(root_key.clone())];

        while let Some(item) = work.pop() {
            match item {
                Work::Visit(key) => {
                    if memo_keys.contains(&key) {
                        stats.memo_hits += 1;
                        continue;
                    }
                    if results.contains_key(&key) {
                        continue;
                    }
                    if let Some(result) = Self::terminal_result(&key, &zero) {
                        results.insert(key, result);
                        continue;
                    }
                    if let Some(result) = self.shared_cache_get(&key, &mut stats) {
                        results.insert(key, result);
                        continue;
                    }
                    if memo_keys.len() == memo_limit {
                        stats.peak_memo_entries = memo_keys.len();
                        return Err(ApplyError::MemoLimit {
                            attempted: memo_keys.len().saturating_add(1),
                            stats,
                        });
                    }
                    memo_keys.insert(key.clone());
                    stats.peak_memo_entries = stats.peak_memo_entries.max(memo_keys.len());

                    let left_view = self.view(&key.left);
                    let right_view = self.view(&key.right);
                    let top = match (&left_view, &right_view) {
                        (RootView::Node { variable, .. }, RootView::Terminal)
                        | (RootView::Terminal, RootView::Node { variable, .. }) => *variable,
                        (
                            RootView::Node { variable: left, .. },
                            RootView::Node {
                                variable: right, ..
                            },
                        ) => (*left).min(*right),
                        (RootView::Terminal, RootView::Terminal) => {
                            unreachable!("terminal pairs are handled by terminal_result")
                        }
                    };
                    let (left_hi, left_lo) = Self::cofactors_at(left_view, top, &key.left, &zero);
                    let (right_hi, right_lo) =
                        Self::cofactors_at(right_view, top, &key.right, &zero);
                    let hi = Self::key(op, left_hi, right_hi);
                    let lo = Self::key(op, left_lo, right_lo);
                    work.push(Work::Combine {
                        key,
                        variable: top,
                        hi: hi.clone(),
                        lo: lo.clone(),
                    });
                    work.push(Work::Visit(hi));
                    work.push(Work::Visit(lo));
                }
                Work::Combine {
                    key,
                    variable,
                    hi,
                    lo,
                } => {
                    let hi = results
                        .get(&hi)
                        .expect("HI result is computed before its parent")
                        .clone();
                    let lo = results
                        .get(&lo)
                        .expect("LO result is computed before its parent")
                        .clone();
                    let result = match self.make_node(variable, &hi, &lo) {
                        Ok((result, created)) => {
                            stats.nodes_created += usize::from(created);
                            result
                        }
                        Err(()) => return Err(ApplyError::NodeLimit { stats }),
                    };
                    self.shared_cache_insert(key.clone(), result.clone());
                    results.insert(key, result);
                }
            }
        }

        Ok((
            results
                .remove(&root_key)
                .expect("the root result is computed by the work stack"),
            stats,
        ))
    }
}
