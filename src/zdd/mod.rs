//! Internal adapter around the selected ZDD backend.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
use oxidd::util::SatCountCache;
use oxidd::zbdd::{ZBDDFunction, ZBDDManagerRef};
use oxidd::{
    BooleanFunction, BooleanVecSet, Function, HasLevel, InnerNode, Manager, ManagerRef, Node,
};

const TERMINAL_NODES: usize = 2;
const MAX_INNER_NODES: usize = (u32::MAX as usize) - (TERMINAL_NODES - 1);

#[derive(Debug)]
pub(crate) enum CreateError {
    TooManyVariables,
    NodeCapacityTooLarge,
}

#[derive(Debug)]
pub(crate) struct BuildError {
    pub(crate) nodes_created: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ApplyOp {
    Union,
    Intersection,
    Difference,
    SymmetricDifference,
}

#[derive(Debug)]
pub(crate) enum ApplyError {
    MemoLimit { attempted: usize, stats: ApplyStats },
    NodeLimit { stats: ApplyStats },
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ApplyStats {
    pub(crate) nodes_created: usize,
    pub(crate) peak_memo_entries: usize,
    pub(crate) memo_hits: usize,
    pub(crate) shared_cache_hits: usize,
    pub(crate) shared_cache_misses: usize,
}

pub(crate) enum FilterSpec<'a> {
    Contains(u32),
    Excludes(u32),
    Subsets(&'a [u32]),
    Supersets(&'a [u32]),
    Cardinality { lower: usize, upper: usize },
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ManagerStats {
    pub(crate) peak_live_nodes: usize,
    pub(crate) nodes_created: usize,
    pub(crate) shared_cache_entries: usize,
    pub(crate) shared_cache_hits: usize,
    pub(crate) shared_cache_misses: usize,
    pub(crate) shared_cache_evictions: usize,
    pub(crate) gc_count: u64,
}

#[derive(Default)]
struct TrieNode {
    terminal: bool,
    children: Vec<(u32, usize)>,
}

/// Owns the backend manager and all roots required by the fixed universe.
pub(crate) struct ZddManager {
    manager: ZBDDManagerRef,
    variables: Vec<ZBDDFunction>,
    powerset: ZBDDFunction,
    shared_cache: Mutex<SharedCache>,
    peak_live_nodes: AtomicUsize,
    nodes_created: AtomicUsize,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Root(ZBDDFunction);

#[derive(Clone, PartialEq, Eq, Hash)]
struct ApplyKey {
    op: ApplyOp,
    left: Root,
    right: Root,
}

struct SharedCache {
    capacity: usize,
    entries: HashMap<ApplyKey, Root>,
    insertion_order: VecDeque<ApplyKey>,
    hits: usize,
    misses: usize,
    evictions: usize,
}

enum RootView {
    Terminal,
    Node { variable: u32, hi: Root, lo: Root },
}

enum Work {
    Visit(ApplyKey),
    Combine {
        key: ApplyKey,
        variable: u32,
        hi: ApplyKey,
        lo: ApplyKey,
    },
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct FilterKey {
    root: Root,
    position: usize,
    lower: usize,
    upper: usize,
}

enum FilterWork {
    Visit(FilterKey),
    MakeNode {
        key: FilterKey,
        variable: u32,
        hi: FilterKey,
        lo: FilterKey,
    },
    Alias {
        key: FilterKey,
        child: FilterKey,
    },
}

impl ZddManager {
    pub(crate) fn new(
        variable_count: usize,
        node_capacity: usize,
        shared_cache_entries: usize,
    ) -> Result<Self, CreateError> {
        let variable_count_u32 =
            u32::try_from(variable_count).map_err(|_| CreateError::TooManyVariables)?;
        if node_capacity > MAX_INNER_NODES {
            return Err(CreateError::NodeCapacityTooLarge);
        }
        // The backend apply cache is deliberately disabled. Its direct-mapped
        // implementation rounds capacities up to a power of two and treats
        // zero as one entry, which cannot implement the public cache limit.
        let manager = oxidd::zbdd::new_manager(node_capacity, 0, 1);
        let (variables, powerset) = manager.with_manager_exclusive(|backend| {
            backend.add_vars(variable_count_u32);

            // Adding variables creates the backend's tautology chain. Keeping
            // every singleton root here fixes the universe and ensures later
            // node construction never needs a second variable representation.
            let variables = (0..variable_count_u32)
                .map(|variable| {
                    ZBDDFunction::singleton(backend, variable)
                        .expect("the caller reserved the complete universe capacity")
                })
                .collect();
            let powerset = ZBDDFunction::t(backend);
            (variables, powerset)
        });

        let initial_nodes = manager.with_manager_shared(|backend| backend.num_inner_nodes());
        Ok(Self {
            manager,
            variables,
            powerset,
            shared_cache: Mutex::new(SharedCache {
                capacity: shared_cache_entries,
                entries: HashMap::new(),
                insertion_order: VecDeque::new(),
                hits: 0,
                misses: 0,
                evictions: 0,
            }),
            peak_live_nodes: AtomicUsize::new(initial_nodes),
            nodes_created: AtomicUsize::new(initial_nodes),
        })
    }

    pub(crate) fn empty(&self) -> Root {
        Root(self.manager.with_manager_shared(ZBDDFunction::empty))
    }

    pub(crate) fn unit(&self) -> Root {
        Root(self.manager.with_manager_shared(ZBDDFunction::base))
    }

    pub(crate) fn powerset(&self) -> Root {
        Root(self.powerset.clone())
    }

    pub(crate) fn build_from_sets(&self, sets: &[Vec<u32>]) -> Result<(Root, usize), BuildError> {
        let mut trie = vec![TrieNode::default()];
        for set in sets {
            let mut node_index = 0;
            for &variable in set {
                let child_index = match trie[node_index]
                    .children
                    .binary_search_by_key(&variable, |&(child_variable, _)| child_variable)
                {
                    Ok(position) => trie[node_index].children[position].1,
                    Err(position) => {
                        let child_index = trie.len();
                        trie.push(TrieNode::default());
                        trie[node_index]
                            .children
                            .insert(position, (variable, child_index));
                        child_index
                    }
                };
                node_index = child_index;
            }
            trie[node_index].terminal = true;
        }

        self.manager.with_manager_shared(|backend| {
            let mut roots: Vec<Option<ZBDDFunction>> = (0..trie.len()).map(|_| None).collect();
            let mut nodes_created = 0;

            // Children are appended after their parents, so reverse index order
            // is a non-recursive post-order traversal of the trie.
            for node_index in (0..trie.len()).rev() {
                let node = &trie[node_index];
                let mut edge = if node.terminal {
                    ZBDDFunction::base_edge(backend)
                } else {
                    ZBDDFunction::empty_edge(backend)
                };

                // Lower variable indices are higher in the ZDD. Building
                // siblings in reverse order lets each new node use the
                // already-built higher-index siblings as its LO branch.
                for &(variable, child_index) in node.children.iter().rev() {
                    let nodes_before = backend.num_inner_nodes();
                    let gc_before = backend.gc_count();
                    let hi = backend.clone_edge(
                        roots[child_index]
                            .as_ref()
                            .expect("trie children are built before their parents")
                            .as_edge(backend),
                    );
                    edge = match oxidd::zbdd::make_node(
                        backend,
                        self.variables[variable as usize].as_edge(backend),
                        hi,
                        edge,
                    ) {
                        Ok(edge) => edge,
                        Err(_) => return Err(BuildError { nodes_created }),
                    };
                    if backend.gc_count() != gc_before || backend.num_inner_nodes() > nodes_before {
                        nodes_created += 1;
                        self.record_node_created(backend.num_inner_nodes());
                    }
                }
                roots[node_index] = Some(ZBDDFunction::from_edge(backend, edge));
            }

            let root = roots[0]
                .take()
                .expect("the trie always contains its root node");
            Ok((Root(root), nodes_created))
        })
    }

    pub(crate) fn roots_equal(&self, left: &Root, right: &Root) -> bool {
        left.0 == right.0
    }

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

    pub(crate) fn contains(&self, root: &Root, set: &[u32]) -> bool {
        let zero = self.empty();
        let one = self.unit();
        let mut remainder = root.clone();
        let mut set_index = 0;

        loop {
            if self.roots_equal(&remainder, &zero) {
                return false;
            }
            match self.view(&remainder) {
                RootView::Terminal => {
                    return set_index == set.len() && self.roots_equal(&remainder, &one);
                }
                RootView::Node { variable, hi, lo } => {
                    if set
                        .get(set_index)
                        .is_some_and(|candidate| *candidate < variable)
                    {
                        return false;
                    }
                    if set.get(set_index) == Some(&variable) {
                        remainder = hi;
                        set_index += 1;
                    } else {
                        remainder = lo;
                    }
                }
            }
        }
    }

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

    pub(crate) fn stats(&self) -> ManagerStats {
        let cache = self
            .shared_cache
            .lock()
            .expect("shared cache lock poisoned");
        ManagerStats {
            peak_live_nodes: self.peak_live_nodes.load(Ordering::Relaxed),
            nodes_created: self.nodes_created.load(Ordering::Relaxed),
            shared_cache_entries: cache.entries.len(),
            shared_cache_hits: cache.hits,
            shared_cache_misses: cache.misses,
            shared_cache_evictions: cache.evictions,
            gc_count: self
                .manager
                .with_manager_shared(|backend| backend.gc_count()),
        }
    }

    fn key(op: ApplyOp, mut left: Root, mut right: Root) -> ApplyKey {
        if op != ApplyOp::Difference && right < left {
            std::mem::swap(&mut left, &mut right);
        }
        ApplyKey { op, left, right }
    }

    fn terminal_result(key: &ApplyKey, zero: &Root) -> Option<Root> {
        let left_zero = key.left == *zero;
        let right_zero = key.right == *zero;
        let equal = key.left == key.right;
        match key.op {
            ApplyOp::Union => {
                if left_zero {
                    Some(key.right.clone())
                } else if right_zero || equal {
                    Some(key.left.clone())
                } else {
                    None
                }
            }
            ApplyOp::Intersection => {
                if left_zero || right_zero {
                    Some(zero.clone())
                } else if equal {
                    Some(key.left.clone())
                } else {
                    None
                }
            }
            ApplyOp::Difference => {
                if left_zero || equal {
                    Some(zero.clone())
                } else if right_zero {
                    Some(key.left.clone())
                } else {
                    None
                }
            }
            ApplyOp::SymmetricDifference => {
                if equal {
                    Some(zero.clone())
                } else if left_zero {
                    Some(key.right.clone())
                } else if right_zero {
                    Some(key.left.clone())
                } else {
                    None
                }
            }
        }
    }

    fn view(&self, root: &Root) -> RootView {
        self.manager.with_manager_shared(|backend| {
            match backend.get_node(root.0.as_edge(backend)) {
                Node::Terminal(_) => RootView::Terminal,
                Node::Inner(node) => {
                    let variable = backend.level_to_var(node.level());
                    let mut children = node.children();
                    let hi = ZBDDFunction::from_edge(
                        backend,
                        backend.clone_edge(&children.next().unwrap()),
                    );
                    let lo = ZBDDFunction::from_edge(
                        backend,
                        backend.clone_edge(&children.next().unwrap()),
                    );
                    RootView::Node {
                        variable,
                        hi: Root(hi),
                        lo: Root(lo),
                    }
                }
            }
        })
    }

    fn cofactors_at(view: RootView, variable: u32, root: &Root, zero: &Root) -> (Root, Root) {
        match view {
            RootView::Node {
                variable: own,
                hi,
                lo,
            } if own == variable => (hi, lo),
            RootView::Node { .. } | RootView::Terminal => (zero.clone(), root.clone()),
        }
    }

    fn make_node(&self, variable: u32, hi: &Root, lo: &Root) -> Result<(Root, bool), ()> {
        self.manager.with_manager_shared(|backend| {
            let before = backend.num_inner_nodes();
            let gc_before = backend.gc_count();
            let hi = backend.clone_edge(hi.0.as_edge(backend));
            let lo = backend.clone_edge(lo.0.as_edge(backend));
            let edge = oxidd::zbdd::make_node(
                backend,
                self.variables[variable as usize].as_edge(backend),
                hi,
                lo,
            )
            .map_err(|_| ())?;
            let created = backend.gc_count() != gc_before || backend.num_inner_nodes() > before;
            if created {
                self.record_node_created(backend.num_inner_nodes());
            }
            Ok((Root(ZBDDFunction::from_edge(backend, edge)), created))
        })
    }

    fn record_node_created(&self, live_nodes: usize) {
        self.nodes_created.fetch_add(1, Ordering::Relaxed);
        self.peak_live_nodes
            .fetch_max(live_nodes, Ordering::Relaxed);
    }

    fn shared_cache_get(&self, key: &ApplyKey, stats: &mut ApplyStats) -> Option<Root> {
        let mut cache = self
            .shared_cache
            .lock()
            .expect("shared cache lock poisoned");
        if cache.capacity == 0 {
            return None;
        }
        if let Some(result) = cache.entries.get(key).cloned() {
            cache.hits += 1;
            stats.shared_cache_hits += 1;
            Some(result)
        } else {
            cache.misses += 1;
            stats.shared_cache_misses += 1;
            None
        }
    }

    fn shared_cache_insert(&self, key: ApplyKey, result: Root) {
        let mut cache = self
            .shared_cache
            .lock()
            .expect("shared cache lock poisoned");
        if cache.capacity == 0 || cache.entries.contains_key(&key) {
            return;
        }
        if cache.entries.len() == cache.capacity
            && let Some(evicted) = cache.insertion_order.pop_front()
        {
            cache.entries.remove(&evicted);
            cache.evictions += 1;
        }
        cache.insertion_order.push_back(key.clone());
        cache.entries.insert(key, result);
    }

    pub(crate) fn inner_node_count(&self) -> usize {
        self.manager
            .with_manager_shared(|backend| backend.num_inner_nodes())
    }

    #[cfg(test)]
    fn count(&self, root: &Root) -> u128 {
        let mut cache = SatCountCache::<u128, std::collections::hash_map::RandomState>::default();
        root.0.sat_count(self.variables.len() as u32, &mut cache)
    }

    /// Serializes a reachable DAG using deterministic traversal-local IDs.
    /// Backend node IDs and hash-table iteration order are never observed.
    #[cfg(test)]
    pub(crate) fn normalized_snapshot(&self, root: &Root) -> String {
        fn reference(
            manager: &ZddManager,
            root: Root,
            zero: &Root,
            one: &Root,
            nodes: &mut Vec<Root>,
        ) -> String {
            if manager.roots_equal(&root, zero) {
                return "ZERO".to_owned();
            }
            if manager.roots_equal(&root, one) {
                return "ONE".to_owned();
            }
            if let Some(index) = nodes
                .iter()
                .position(|known| manager.roots_equal(known, &root))
            {
                return format!("n{index}");
            }
            let index = nodes.len();
            nodes.push(root);
            format!("n{index}")
        }

        let zero = self.empty();
        let one = self.unit();
        let mut nodes = Vec::new();
        let root_reference = reference(self, root.clone(), &zero, &one, &mut nodes);
        let mut lines = vec![format!("root={root_reference}")];
        let mut index = 0;
        while index < nodes.len() {
            let RootView::Node { variable, hi, lo } = self.view(&nodes[index]) else {
                unreachable!("terminals are represented by names, not local node IDs");
            };
            let hi = reference(self, hi, &zero, &one, &mut nodes);
            let lo = reference(self, lo, &zero, &one, &mut nodes);
            lines.push(format!("n{index}: variable={variable}, hi={hi}, lo={lo}"));
            index += 1;
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals_and_powerset_have_zdd_semantics() {
        let manager = ZddManager::new(3, 64, 0).unwrap();
        assert_eq!(manager.count(&manager.empty()), 0);
        assert_eq!(manager.count(&manager.unit()), 1);
        assert_eq!(manager.count(&manager.powerset()), 8);
        assert!(!manager.roots_equal(&manager.empty(), &manager.unit()));
    }

    #[test]
    fn empty_universe_powerset_is_unit() {
        let manager = ZddManager::new(0, 0, 0).unwrap();
        assert!(manager.roots_equal(&manager.powerset(), &manager.unit()));
    }

    #[test]
    fn unique_table_reuses_nodes_and_lo_equals_hi_is_preserved() {
        let manager = ZddManager::new(1, 16, 0).unwrap();
        let before = manager.inner_node_count();
        let first = manager.build_from_sets(&[vec![], vec![0]]).unwrap().0;
        let after_first = manager.inner_node_count();
        let second = manager.build_from_sets(&[vec![0], vec![]]).unwrap().0;

        assert_eq!(manager.count(&first), 2);
        assert!(manager.roots_equal(&first, &second));
        assert!(after_first >= before);
        assert_eq!(manager.inner_node_count(), after_first);
        assert!(!manager.roots_equal(&first, &manager.unit()));
    }

    #[test]
    fn table_growth_keeps_canonical_roots() {
        let manager = ZddManager::new(10, 4_096, 0).unwrap();
        let sets: Vec<Vec<u32>> = (0u32..512)
            .map(|bits| (0..10).filter(|v| bits & (1 << v) != 0).collect())
            .collect();
        let first = manager.build_from_sets(&sets).unwrap().0;
        let second = manager.build_from_sets(&sets).unwrap().0;
        assert!(manager.roots_equal(&first, &second));
        assert_eq!(manager.count(&first), 512);
    }

    #[test]
    fn explicit_sets_match_an_independent_membership_oracle() {
        let manager = ZddManager::new(3, 64, 0).unwrap();
        let expected = [vec![], vec![0], vec![0, 2], vec![1, 2]];
        let root = manager.build_from_sets(&expected).unwrap().0;

        for bits in 0u32..8 {
            let candidate: Vec<u32> = (0..3)
                .filter(|variable| bits & (1 << variable) != 0)
                .collect();
            assert_eq!(
                manager.contains(&root, &candidate),
                expected.contains(&candidate),
                "membership differed for {candidate:?}"
            );
        }
    }

    #[test]
    fn normalized_snapshot_is_independent_of_backend_node_allocation() {
        let first = ZddManager::new(3, 64, 0).unwrap();
        let first_root = first
            .build_from_sets(&[vec![], vec![0], vec![0, 1], vec![2]])
            .unwrap()
            .0;

        let second = ZddManager::new(3, 64, 0).unwrap();
        let temporary = second
            .build_from_sets(&[vec![0, 2], vec![1], vec![1, 2]])
            .unwrap()
            .0;
        let second_root = second
            .build_from_sets(&[vec![2], vec![0, 1], vec![], vec![0]])
            .unwrap()
            .0;
        drop(temporary);

        let expected = concat!(
            "root=n0\n",
            "n0: variable=0, hi=n1, lo=n2\n",
            "n1: variable=1, hi=ONE, lo=ONE\n",
            "n2: variable=2, hi=ONE, lo=ONE",
        );
        assert_eq!(first.normalized_snapshot(&first_root), expected);
        assert_eq!(second.normalized_snapshot(&second_root), expected);
    }
}
