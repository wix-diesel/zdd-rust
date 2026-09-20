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

mod apply;
mod construction;
mod filter;
mod import;
mod query;
mod snapshot;
mod support;

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

pub(crate) const QUERY_ZERO: usize = 0;
pub(crate) const QUERY_ONE: usize = 1;

#[derive(Clone, Debug)]
pub(crate) struct QueryNode {
    pub(crate) variable: u32,
    pub(crate) hi: usize,
    pub(crate) lo: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct QueryDag {
    pub(crate) root: usize,
    pub(crate) nodes: Vec<QueryNode>,
}

pub(crate) struct TransferDag {
    pub(crate) roots: Vec<usize>,
    pub(crate) nodes: Vec<QueryNode>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SnapshotLimitError {
    pub(crate) attempted: usize,
    pub(crate) snapshot_nodes: usize,
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

#[cfg(test)]
#[path = "../../tests/internal/zdd.rs"]
mod tests;
