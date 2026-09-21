#![cfg(feature = "graph")]

use std::collections::BTreeSet;

use zdd_family::{EdgeOrder, FamilySpace, Graph, GraphSpace, SetFamily};

fn masks(text: &str) -> BTreeSet<u64> {
    if text == "-" || text.is_empty() {
        return BTreeSet::new();
    }
    text.split(',')
        .map(|value| u64::from_str_radix(value, 16).unwrap())
        .collect()
}

fn family_from_masks(
    space: &FamilySpace,
    variable_count: usize,
    values: &BTreeSet<u64>,
) -> SetFamily {
    let sets = values.iter().map(|mask| {
        (0..variable_count)
            .filter_map(|index| {
                (mask & (1 << index) != 0).then_some(space.variable(index).unwrap())
            })
            .collect::<Vec<_>>()
    });
    space.from_sets(sets).unwrap()
}

fn family_masks(family: &SetFamily) -> BTreeSet<u64> {
    family
        .iter()
        .map(|solution| {
            solution
                .iter()
                .fold(0, |mask, variable| mask | (1 << variable.index()))
        })
        .collect()
}

fn edge_masks(family: &zdd_family::EdgeFamily) -> BTreeSet<u64> {
    family
        .iter()
        .map(|solution| {
            solution
                .iter()
                .fold(0, |mask, edge| mask | (1 << edge.index()))
        })
        .collect()
}

#[test]
fn public_api_matches_committed_cross_implementation_fixtures() {
    for line in include_str!("../tools/differential/fixtures.tsv").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        assert_eq!(fields.len(), 9, "malformed fixture: {line}");
        let [name, kind, size, edges, order, args, left, right, expected] =
            fields.as_slice()
        else {
            unreachable!()
        };
        let size = size.parse::<usize>().unwrap();
        let expected = masks(expected);

        if kind.starts_with("family-") {
            let space = FamilySpace::new(size).unwrap();
            let left = family_from_masks(&space, size, &masks(left));
            let right = family_from_masks(&space, size, &masks(right));
            let actual = match *kind {
                "family-union" => left.union(&right),
                "family-intersection" => left.intersection(&right),
                "family-difference" => left.difference(&right),
                _ => panic!("unknown fixture kind {kind}"),
            }
            .unwrap();
            assert_eq!(family_masks(&actual), expected, "case {name}");
            continue;
        }

        let edge_pairs = edges.split(',').map(|edge| {
            let (first, second) = edge.split_once('-').unwrap();
            (first.parse::<usize>().unwrap(), second.parse::<usize>().unwrap())
        });
        let graph = Graph::from_edges(size, edge_pairs).unwrap();
        let edge_order = EdgeOrder::new(
            &graph,
            order
                .split(',')
                .map(|index| graph.edge_id(index.parse().unwrap()).unwrap()),
        )
        .unwrap();
        let space = GraphSpace::builder(&graph)
            .ordering(edge_order)
            .build()
            .unwrap();
        let actual = match *kind {
            "matchings" => space.matchings().unwrap(),
            "cycles" => space.cycles().unwrap(),
            "paths" => {
                let (source, target) = args.split_once(',').unwrap();
                space
                    .paths(
                        graph.vertex_id(source.parse().unwrap()).unwrap(),
                        graph.vertex_id(target.parse().unwrap()).unwrap(),
                    )
                    .unwrap()
            }
            _ => panic!("unknown fixture kind {kind}"),
        };
        assert_eq!(edge_masks(&actual), expected, "case {name}");
    }
}
