//! Shared byte-driven fuzz scenarios and their independent explicit-set oracles.

use std::collections::{BTreeSet, VecDeque};

use zdd_family::{
    BfsOrder, EdgeFamily, EdgeOrder, Error, FamilySpace, Graph, GraphSpace, Limits, QueryLimits,
    SetFamily,
};

type Masks = BTreeSet<u64>;

struct Bytes<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Bytes<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn next(&mut self) -> u8 {
        let value = self.data.get(self.position).copied().unwrap_or(0);
        self.position = self.position.saturating_add(1);
        value
    }
}

fn variables(space: &FamilySpace, count: usize) -> Vec<zdd_family::VariableId> {
    (0..count)
        .map(|index| space.variable(index).unwrap())
        .collect()
}

fn family_masks(family: &SetFamily) -> Masks {
    family
        .iter()
        .map(|solution| {
            solution
                .iter()
                .fold(0u64, |mask, variable| mask | (1 << variable.index()))
        })
        .collect()
}

fn edge_masks(family: &EdgeFamily) -> Masks {
    family
        .iter()
        .map(|solution| {
            solution
                .iter()
                .fold(0u64, |mask, edge| mask | (1 << edge.index()))
        })
        .collect()
}

fn assert_family(family: &SetFamily, expected: &Masks) {
    assert_eq!(family_masks(family), *expected);
    assert_eq!(family.count(), expected.len().into());
}

fn explicit_family(
    space: &FamilySpace,
    vars: &[zdd_family::VariableId],
    bytes: &mut Bytes<'_>,
) -> (SetFamily, Masks) {
    let set_count = usize::from(bytes.next() % 17);
    let mut expected = Masks::new();
    let mut sets = Vec::with_capacity(set_count);
    for _ in 0..set_count {
        let mask = u64::from(bytes.next()) & ((1u64 << vars.len()) - 1);
        expected.insert(mask);
        let mut set = Vec::new();
        for (index, &variable) in vars.iter().enumerate().rev() {
            if mask & (1 << index) != 0 {
                set.push(variable);
                if bytes.next() & 1 != 0 {
                    set.push(variable);
                }
            }
        }
        sets.push(set);
    }
    (space.from_sets(sets).unwrap(), expected)
}

/// Exercises bounded sequences of public set-family operations.
pub fn run_family_operations(data: &[u8]) {
    let mut bytes = Bytes::new(data);
    let variable_count = usize::from(bytes.next() % 7);
    let space = FamilySpace::builder(variable_count)
        .limits(Limits {
            max_live_nodes: 4_096,
            max_operation_memo_entries: 1_024,
            shared_cache_entries: usize::from(bytes.next() % 17),
            ..Limits::default()
        })
        .build()
        .unwrap();
    let vars = variables(&space, variable_count);
    let all: Masks = (0..(1u64 << variable_count)).collect();
    let (explicit, explicit_masks) = explicit_family(&space, &vars, &mut bytes);
    let mut families = vec![
        space.empty(),
        space.unit(),
        space.powerset().unwrap(),
        explicit,
    ];
    let mut oracles = vec![Masks::new(), Masks::from([0]), all, explicit_masks];

    let steps = usize::from(bytes.next() % 65);
    for _ in 0..steps {
        let left = usize::from(bytes.next()) % families.len();
        let right = usize::from(bytes.next()) % families.len();
        let opcode = bytes.next() % 12;
        let (result, expected) = match opcode {
            0 => (
                families[left].union(&families[right]).unwrap(),
                oracles[left].union(&oracles[right]).copied().collect(),
            ),
            1 => (
                families[left].intersection(&families[right]).unwrap(),
                oracles[left]
                    .intersection(&oracles[right])
                    .copied()
                    .collect(),
            ),
            2 => (
                families[left].difference(&families[right]).unwrap(),
                oracles[left].difference(&oracles[right]).copied().collect(),
            ),
            3 => (
                families[left]
                    .symmetric_difference(&families[right])
                    .unwrap(),
                oracles[left]
                    .symmetric_difference(&oracles[right])
                    .copied()
                    .collect(),
            ),
            4 if !vars.is_empty() => {
                let index = usize::from(bytes.next()) % vars.len();
                (
                    families[left].filter_contains(vars[index]).unwrap(),
                    oracles[left]
                        .iter()
                        .filter(|mask| **mask & (1 << index) != 0)
                        .copied()
                        .collect(),
                )
            }
            5 if !vars.is_empty() => {
                let index = usize::from(bytes.next()) % vars.len();
                (
                    families[left].filter_excludes(vars[index]).unwrap(),
                    oracles[left]
                        .iter()
                        .filter(|mask| **mask & (1 << index) == 0)
                        .copied()
                        .collect(),
                )
            }
            6 => {
                let count = usize::from(bytes.next() % 9);
                (
                    families[left].cardinality().exactly(count).unwrap(),
                    oracles[left]
                        .iter()
                        .filter(|mask| mask.count_ones() as usize == count)
                        .copied()
                        .collect(),
                )
            }
            7 => {
                let lower = usize::from(bytes.next() % 9);
                let upper = usize::from(bytes.next() % 9);
                if lower > upper {
                    assert!(matches!(
                        families[left].cardinality().between(lower..=upper),
                        Err(Error::InvalidRange { .. })
                    ));
                    assert_family(&families[left], &oracles[left]);
                    continue;
                }
                (
                    families[left].cardinality().between(lower..=upper).unwrap(),
                    oracles[left]
                        .iter()
                        .filter(|mask| {
                            let count = mask.count_ones() as usize;
                            (lower..=upper).contains(&count)
                        })
                        .copied()
                        .collect(),
                )
            }
            8 => {
                let selected = u64::from(bytes.next()) & ((1u64 << variable_count) - 1);
                let elements = vars
                    .iter()
                    .enumerate()
                    .filter_map(|(index, variable)| {
                        (selected & (1 << index) != 0).then_some(*variable)
                    })
                    .collect::<Vec<_>>();
                (
                    families[left].filter_subsets_of(&elements).unwrap(),
                    oracles[left]
                        .iter()
                        .filter(|mask| **mask & !selected == 0)
                        .copied()
                        .collect(),
                )
            }
            9 => {
                let selected = u64::from(bytes.next()) & ((1u64 << variable_count) - 1);
                let elements = vars
                    .iter()
                    .enumerate()
                    .filter_map(|(index, variable)| {
                        (selected & (1 << index) != 0).then_some(*variable)
                    })
                    .collect::<Vec<_>>();
                (
                    families[left].filter_supersets_of(&elements).unwrap(),
                    oracles[left]
                        .iter()
                        .filter(|mask| **mask & selected == selected)
                        .copied()
                        .collect(),
                )
            }
            _ => {
                assert_eq!(
                    families[left].is_subset_of(&families[right]).unwrap(),
                    oracles[left].is_subset(&oracles[right])
                );
                assert_family(&families[left], &oracles[left]);
                continue;
            }
        };
        assert_family(&result, &expected);
        families.push(result);
        oracles.push(expected);
        if families.len() > 24 {
            families.remove(4);
            oracles.remove(4);
        }
    }
}

fn matching_oracle(graph: &Graph) -> Masks {
    (0..(1u64 << graph.edge_count()))
        .filter(|mask| {
            let mut used = vec![false; graph.vertex_count()];
            for index in 0..graph.edge_count() {
                if mask & (1 << index) == 0 {
                    continue;
                }
                let (a, b) = graph.endpoints(graph.edge_id(index).unwrap()).unwrap();
                let first_was_used = std::mem::replace(&mut used[a.index()], true);
                let second_was_used = std::mem::replace(&mut used[b.index()], true);
                if first_was_used || second_was_used {
                    return false;
                }
            }
            true
        })
        .collect()
}

fn connected_selected(graph: &Graph, degree: &[usize], adjacency: &[Vec<usize>]) -> bool {
    let Some(start) = degree.iter().position(|degree| *degree != 0) else {
        return false;
    };
    let mut reached = vec![false; graph.vertex_count()];
    let mut queue = VecDeque::from([start]);
    reached[start] = true;
    while let Some(vertex) = queue.pop_front() {
        for &neighbor in &adjacency[vertex] {
            if !reached[neighbor] {
                reached[neighbor] = true;
                queue.push_back(neighbor);
            }
        }
    }
    degree
        .iter()
        .enumerate()
        .all(|(vertex, degree)| *degree == 0 || reached[vertex])
}

fn degrees(graph: &Graph, mask: u64) -> (Vec<usize>, Vec<Vec<usize>>) {
    let mut degree = vec![0; graph.vertex_count()];
    let mut adjacency = vec![Vec::new(); graph.vertex_count()];
    for index in 0..graph.edge_count() {
        if mask & (1 << index) == 0 {
            continue;
        }
        let (a, b) = graph.endpoints(graph.edge_id(index).unwrap()).unwrap();
        degree[a.index()] += 1;
        degree[b.index()] += 1;
        adjacency[a.index()].push(b.index());
        adjacency[b.index()].push(a.index());
    }
    (degree, adjacency)
}

fn cycle_oracle(graph: &Graph) -> Masks {
    (1..(1u64 << graph.edge_count()))
        .filter(|mask| {
            let (degree, adjacency) = degrees(graph, *mask);
            degree.iter().all(|degree| *degree == 0 || *degree == 2)
                && connected_selected(graph, &degree, &adjacency)
        })
        .collect()
}

fn path_oracle(graph: &Graph, source: usize, target: usize) -> Masks {
    (0..(1u64 << graph.edge_count()))
        .filter(|mask| {
            let (degree, adjacency) = degrees(graph, *mask);
            if degree[source] != 1 || degree[target] != 1 {
                return false;
            }
            if degree.iter().enumerate().any(|(vertex, degree)| {
                vertex != source && vertex != target && *degree != 0 && *degree != 2
            }) {
                return false;
            }
            connected_selected(graph, &degree, &adjacency)
        })
        .collect()
}

/// Exercises graph validation, edge ordering, ID maps, and graph algorithms.
pub fn run_graph_inputs(data: &[u8]) {
    let mut bytes = Bytes::new(data);
    let vertex_count = usize::from(bytes.next() % 6);
    let raw_count = usize::from(bytes.next() % 13);
    let raw_edges = (0..raw_count)
        .map(|_| (usize::from(bytes.next() % 7), usize::from(bytes.next() % 7)))
        .collect::<Vec<_>>();
    let _ = Graph::from_edges(vertex_count, raw_edges);

    let candidates = (0..vertex_count)
        .flat_map(|first| ((first + 1)..vertex_count).map(move |second| (first, second)))
        .collect::<Vec<_>>();
    let selection = u16::from_le_bytes([bytes.next(), bytes.next()]);
    let edges = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, edge)| (selection & (1 << index) != 0).then_some(*edge));
    let graph = Graph::from_edges(vertex_count, edges).unwrap();

    if graph.edge_count() > 0 {
        let duplicate = graph.edge_id(0).unwrap();
        assert!(EdgeOrder::new(&graph, [duplicate, duplicate]).is_err());
    }
    let reverse = EdgeOrder::new(
        &graph,
        (0..graph.edge_count())
            .rev()
            .map(|index| graph.edge_id(index).unwrap()),
    )
    .unwrap();
    let spaces = [
        GraphSpace::new(&graph).unwrap(),
        GraphSpace::builder(&graph)
            .ordering(BfsOrder)
            .build()
            .unwrap(),
        GraphSpace::builder(&graph)
            .ordering(reverse)
            .build()
            .unwrap(),
    ];
    let matching = matching_oracle(&graph);
    let cycles = cycle_oracle(&graph);
    for space in &spaces {
        assert_eq!(edge_masks(&space.matchings().unwrap()), matching);
        assert_eq!(edge_masks(&space.cycles().unwrap()), cycles);
        for edge_index in 0..graph.edge_count() {
            let edge = graph.edge_id(edge_index).unwrap();
            let variable = space.variable_for_edge(edge).unwrap();
            assert_eq!(space.edge_for_variable(variable).unwrap(), edge);
        }
        if vertex_count >= 2 {
            let source = usize::from(bytes.next()) % vertex_count;
            let mut target = usize::from(bytes.next()) % (vertex_count - 1);
            if target >= source {
                target += 1;
            }
            let actual = space
                .paths(
                    graph.vertex_id(source).unwrap(),
                    graph.vertex_id(target).unwrap(),
                )
                .unwrap();
            assert_eq!(edge_masks(&actual), path_oracle(&graph, source, target));
        }
    }
}

/// Exercises import maps, limits, and successful operations after errors.
pub fn run_import_limits(data: &[u8]) {
    let mut bytes = Bytes::new(data);
    let variable_count = usize::from(bytes.next() % 7);
    let source = FamilySpace::new(variable_count).unwrap();
    let source_vars = variables(&source, variable_count);
    let (family, expected) = explicit_family(&source, &source_vars, &mut bytes);
    let destination = FamilySpace::new(variable_count).unwrap();
    let destination_vars = variables(&destination, variable_count);

    let foreign = FamilySpace::new(variable_count.saturating_add(1)).unwrap();
    let mode = bytes.next() % 5;
    let map = match mode {
        0 => destination_vars.clone(),
        1 => destination_vars
            .iter()
            .copied()
            .take(variable_count.saturating_sub(1))
            .collect(),
        2 if variable_count > 1 => vec![destination_vars[0]; variable_count],
        3 => destination_vars.iter().rev().copied().collect(),
        4 if variable_count > 0 => {
            let mut map = destination_vars.clone();
            map[variable_count - 1] = foreign.variable(variable_count).unwrap();
            map
        }
        _ => destination_vars.clone(),
    };
    match destination.import(&family, &map) {
        Ok(imported) => assert_family(&imported, &expected),
        Err(_) => {
            assert_family(&family, &expected);
            assert!(!destination.unit().is_empty());
            assert!(destination.empty().union(&destination.unit()).is_ok());
        }
    }

    let query_limits = QueryLimits {
        max_snapshot_nodes: usize::from(bytes.next() % 8),
        max_total_count_bits: usize::from(bytes.next() % 32),
    };
    let _ = family.count_index(&query_limits);
    assert_family(&family, &expected);
    assert_eq!(family.count(), expected.len().into());

    let memo_space = FamilySpace::builder(3)
        .limits(Limits {
            max_operation_memo_entries: usize::from(bytes.next() % 8),
            shared_cache_entries: 0,
            ..Limits::default()
        })
        .build()
        .unwrap();
    let memo_vars = variables(&memo_space, 3);
    let left = memo_space
        .from_sets([vec![], vec![memo_vars[0]], vec![memo_vars[1]]])
        .unwrap();
    let right = memo_space
        .from_sets([vec![memo_vars[0]], vec![memo_vars[2]]])
        .unwrap();
    let _ = left.symmetric_difference(&right);
    assert_eq!(family_masks(&left), Masks::from([0, 1, 2]));
    assert_eq!(family_masks(&right), Masks::from([1, 4]));
    assert!(left.union(&memo_space.empty()).is_ok());

    let graph = Graph::from_edges(3, [(0, 1), (1, 2), (0, 2)]).unwrap();
    let graph_space = GraphSpace::builder(&graph)
        .limits(Limits {
            max_frontier_states: usize::from(bytes.next() % 8),
            max_frontier_transitions: usize::from(bytes.next() % 16),
            ..Limits::default()
        })
        .build()
        .unwrap();
    let stable_edges = graph_space.unit();
    if let Ok(matchings) = graph_space.matchings() {
        assert_eq!(edge_masks(&matchings), matching_oracle(&graph));
    }
    assert!(!stable_edges.is_empty());
    assert!(graph_space.powerset().is_ok());

    let constrained = FamilySpace::builder(variable_count)
        .limits(Limits {
            max_live_nodes: variable_count.saturating_mul(2),
            shared_cache_entries: 0,
            ..Limits::default()
        })
        .build()
        .unwrap();
    let stable = constrained.unit();
    if variable_count > 0 {
        let variable = constrained.variable(0).unwrap();
        let _ = constrained.from_sets([vec![], vec![variable]]);
    }
    assert!(!stable.is_empty());
    assert!(
        stable
            .union(&constrained.empty())
            .unwrap()
            .equivalent(&stable)
            .unwrap()
    );
}
