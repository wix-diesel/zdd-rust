use super::*;

impl ZddManager {
    /// Filters a ZDD using an operation-local, iterative DAG dynamic program.
    ///
    /// The `position` field in a key is used by `Contains` and `Supersets`; `lower` and
    /// `upper` are used only by `Cardinality`. Keeping one engine makes the
    /// resource accounting and deep-DAG behavior identical for all filters.
    #[allow(clippy::mutable_key_type)]
    pub(crate) fn filter(
        &self,
        root: &Root,
        spec: FilterSpec<'_>,
        memo_limit: usize,
    ) -> Result<(Root, ApplyStats), ApplyError> {
        let zero = self.empty();
        let one = self.unit();
        let (lower, upper) = match spec {
            FilterSpec::Cardinality { lower, upper } => (lower, upper),
            _ => (0, 0),
        };
        let root_key = FilterKey {
            root: root.clone(),
            position: 0,
            lower,
            upper,
        };
        let mut stats = ApplyStats::default();
        let mut memo_keys = HashSet::new();
        let mut results = HashMap::<FilterKey, Root>::new();
        let mut work = vec![FilterWork::Visit(root_key.clone())];

        while let Some(item) = work.pop() {
            match item {
                FilterWork::Visit(key) => {
                    if memo_keys.contains(&key) {
                        stats.memo_hits += 1;
                        continue;
                    }
                    if results.contains_key(&key) {
                        continue;
                    }

                    if self.roots_equal(&key.root, &zero) {
                        results.insert(key, zero.clone());
                        continue;
                    }
                    if matches!(&spec, FilterSpec::Contains(_)) && key.position == 1 {
                        let result = key.root.clone();
                        results.insert(key, result);
                        continue;
                    }
                    if matches!(&spec, FilterSpec::Supersets(required) if key.position == required.len())
                    {
                        let result = key.root.clone();
                        results.insert(key, result);
                        continue;
                    }
                    if self.roots_equal(&key.root, &one) {
                        let accepted = match &spec {
                            FilterSpec::Contains(_) => false,
                            FilterSpec::Excludes(_) | FilterSpec::Subsets(_) => true,
                            FilterSpec::Supersets(required) => key.position == required.len(),
                            FilterSpec::Cardinality { .. } => key.lower == 0,
                        };
                        results.insert(key, if accepted { one.clone() } else { zero.clone() });
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

                    let RootView::Node { variable, hi, lo } = self.view(&key.root) else {
                        unreachable!("nonterminal roots have an inner node")
                    };
                    let same = |root: Root| FilterKey {
                        root,
                        position: key.position,
                        lower: key.lower,
                        upper: key.upper,
                    };

                    match &spec {
                        FilterSpec::Contains(required) => {
                            if variable > *required {
                                results.insert(key, zero.clone());
                            } else if variable == *required {
                                let hi = FilterKey {
                                    root: hi,
                                    position: 1,
                                    lower: 0,
                                    upper: 0,
                                };
                                let lo = same(zero.clone());
                                work.push(FilterWork::MakeNode {
                                    key,
                                    variable,
                                    hi: hi.clone(),
                                    lo: lo.clone(),
                                });
                                work.push(FilterWork::Visit(hi));
                                work.push(FilterWork::Visit(lo));
                            } else {
                                let hi = same(hi);
                                let lo = same(lo);
                                work.push(FilterWork::MakeNode {
                                    key,
                                    variable,
                                    hi: hi.clone(),
                                    lo: lo.clone(),
                                });
                                work.push(FilterWork::Visit(hi));
                                work.push(FilterWork::Visit(lo));
                            }
                        }
                        FilterSpec::Excludes(excluded) => {
                            if variable > *excluded {
                                let result = key.root.clone();
                                results.insert(key, result);
                            } else if variable == *excluded {
                                let child = same(lo);
                                work.push(FilterWork::Alias {
                                    key,
                                    child: child.clone(),
                                });
                                work.push(FilterWork::Visit(child));
                            } else {
                                let hi = same(hi);
                                let lo = same(lo);
                                work.push(FilterWork::MakeNode {
                                    key,
                                    variable,
                                    hi: hi.clone(),
                                    lo: lo.clone(),
                                });
                                work.push(FilterWork::Visit(hi));
                                work.push(FilterWork::Visit(lo));
                            }
                        }
                        FilterSpec::Subsets(allowed) => {
                            let lo = same(lo);
                            if allowed.binary_search(&variable).is_ok() {
                                let hi = same(hi);
                                work.push(FilterWork::MakeNode {
                                    key,
                                    variable,
                                    hi: hi.clone(),
                                    lo: lo.clone(),
                                });
                                work.push(FilterWork::Visit(hi));
                                work.push(FilterWork::Visit(lo));
                            } else {
                                work.push(FilterWork::Alias {
                                    key,
                                    child: lo.clone(),
                                });
                                work.push(FilterWork::Visit(lo));
                            }
                        }
                        FilterSpec::Supersets(required) => {
                            let required_variable = required[key.position];
                            if required_variable < variable {
                                results.insert(key, zero.clone());
                            } else if required_variable == variable {
                                let hi = FilterKey {
                                    root: hi,
                                    position: key.position + 1,
                                    lower: 0,
                                    upper: 0,
                                };
                                let lo = same(zero.clone());
                                work.push(FilterWork::MakeNode {
                                    key,
                                    variable,
                                    hi: hi.clone(),
                                    lo: lo.clone(),
                                });
                                work.push(FilterWork::Visit(hi));
                                work.push(FilterWork::Visit(lo));
                            } else {
                                let hi = same(hi);
                                let lo = same(lo);
                                work.push(FilterWork::MakeNode {
                                    key,
                                    variable,
                                    hi: hi.clone(),
                                    lo: lo.clone(),
                                });
                                work.push(FilterWork::Visit(hi));
                                work.push(FilterWork::Visit(lo));
                            }
                        }
                        FilterSpec::Cardinality { .. } => {
                            let lo = same(lo);
                            let hi = if key.upper == 0 {
                                FilterKey {
                                    root: zero.clone(),
                                    position: 0,
                                    lower: 0,
                                    upper: 0,
                                }
                            } else {
                                FilterKey {
                                    root: hi,
                                    position: 0,
                                    lower: key.lower.saturating_sub(1),
                                    upper: key.upper - 1,
                                }
                            };
                            work.push(FilterWork::MakeNode {
                                key,
                                variable,
                                hi: hi.clone(),
                                lo: lo.clone(),
                            });
                            work.push(FilterWork::Visit(hi));
                            work.push(FilterWork::Visit(lo));
                        }
                    }
                }
                FilterWork::MakeNode {
                    key,
                    variable,
                    hi,
                    lo,
                } => {
                    let hi = results
                        .get(&hi)
                        .expect("HI filter result is computed before its parent");
                    let lo = results
                        .get(&lo)
                        .expect("LO filter result is computed before its parent");
                    let result = match self.make_node(variable, hi, lo) {
                        Ok((result, created)) => {
                            stats.nodes_created += usize::from(created);
                            result
                        }
                        Err(()) => return Err(ApplyError::NodeLimit { stats }),
                    };
                    results.insert(key, result);
                }
                FilterWork::Alias { key, child } => {
                    let result = results
                        .get(&child)
                        .expect("filter child is computed before its parent")
                        .clone();
                    results.insert(key, result);
                }
            }
        }

        Ok((
            results
                .remove(&root_key)
                .expect("the filter root is computed by the work stack"),
            stats,
        ))
    }
}
