#![cfg(feature = "sampling")]

use std::collections::{BTreeSet, VecDeque};

use rand_core::RngCore;
use zdd_family::{BigUint, FamilySpace, QueryLimits, Solution};

struct BytesRng {
    values: VecDeque<Vec<u8>>,
    fills: usize,
}

impl BytesRng {
    fn new(values: impl IntoIterator<Item = Vec<u8>>) -> Self {
        Self {
            values: values.into_iter().collect(),
            fills: 0,
        }
    }
}

impl RngCore for BytesRng {
    fn next_u32(&mut self) -> u32 {
        panic!("sampling should request the exact byte length")
    }

    fn next_u64(&mut self) -> u64 {
        panic!("sampling should request the exact byte length")
    }

    fn fill_bytes(&mut self, destination: &mut [u8]) {
        self.fills += 1;
        let value = self.values.pop_front().expect("test RNG exhausted");
        assert_eq!(value.len(), destination.len());
        destination.copy_from_slice(&value);
    }
}

#[test]
fn every_rank_of_every_three_variable_family_is_a_bijection() {
    let space = FamilySpace::new(3).unwrap();
    for family_mask in 0u16..(1 << 8) {
        let sets = (0..8)
            .filter(|&set_mask| family_mask & (1 << set_mask) != 0)
            .map(|set_mask| {
                (0..3)
                    .filter(|variable| set_mask & (1 << variable) != 0)
                    .map(|variable| space.variable(variable).unwrap())
                    .collect::<Vec<_>>()
            });
        let family = space.from_sets(sets).unwrap();
        let index = family.count_index(&QueryLimits::default()).unwrap();
        let expected: Vec<_> = family.iter().collect();
        let mut sampled = Vec::new();

        for rank in 0..expected.len() {
            let mut rng = BytesRng::new([vec![rank as u8]]);
            sampled.push(index.sample(&mut rng).unwrap().unwrap());
        }

        assert_eq!(sampled, expected, "family mask {family_mask:#010b}");
        assert_eq!(
            sampled.iter().cloned().collect::<BTreeSet<_>>().len(),
            sampled.len(),
            "family mask {family_mask:#010b}"
        );
    }
}

#[test]
fn empty_unit_and_non_power_of_two_counts_use_exact_rejection_sampling() {
    let space = FamilySpace::new(3).unwrap();
    let mut unused = BytesRng::new([]);
    assert_eq!(space.empty().sample(&mut unused).unwrap(), None);
    assert_eq!(
        space.unit().sample(&mut unused).unwrap(),
        Some(Solution::default())
    );
    assert_eq!(unused.fills, 0);

    let family = space
        .from_sets([
            vec![],
            vec![space.variable(0).unwrap()],
            vec![space.variable(1).unwrap()],
            vec![space.variable(2).unwrap()],
            vec![space.variable(0).unwrap(), space.variable(1).unwrap()],
        ])
        .unwrap();
    let expected = family.iter().nth(4).unwrap();
    let mut rng = BytesRng::new([vec![7], vec![4]]);
    assert_eq!(family.sample(&mut rng).unwrap(), Some(expected));
    assert_eq!(rng.fills, 2, "rank 7 must be rejected for a count of 5");
}

#[test]
fn sampling_supports_counts_larger_than_u128() {
    let space = FamilySpace::new(129).unwrap();
    let family = space.powerset().unwrap();
    let index = family.count_index(&QueryLimits::default()).unwrap();
    assert_eq!(index.count(), &(BigUint::from(1u8) << 129usize));

    let mut highest_rank = vec![u8::MAX; 17];
    highest_rank[16] = 1;
    let mut rng = BytesRng::new([highest_rank]);
    let sample = index.sample(&mut rng).unwrap().unwrap();
    assert_eq!(sample.len(), 129);
    assert!(family.contains(sample.as_slice()).unwrap());
}

struct ReentrantRng {
    family: zdd_family::SetFamily,
    reentries: usize,
}

impl RngCore for ReentrantRng {
    fn next_u32(&mut self) -> u32 {
        panic!("sampling should use fill_bytes")
    }

    fn next_u64(&mut self) -> u64 {
        panic!("sampling should use fill_bytes")
    }

    fn fill_bytes(&mut self, destination: &mut [u8]) {
        assert_eq!(
            self.family.union(&self.family).unwrap().count(),
            BigUint::from(3u8)
        );
        destination.fill(0);
        self.reentries += 1;
    }
}

#[test]
fn rng_can_reenter_the_same_space_without_a_manager_guard() {
    let space = FamilySpace::new(2).unwrap();
    let family = space
        .from_sets([
            vec![],
            vec![space.variable(0).unwrap()],
            vec![space.variable(1).unwrap()],
        ])
        .unwrap();
    let index = family.count_index(&QueryLimits::default()).unwrap();
    let mut rng = ReentrantRng {
        family: family.clone(),
        reentries: 0,
    };

    assert!(index.sample(&mut rng).unwrap().is_some());
    assert!(family.sample(&mut rng).unwrap().is_some());
    assert_eq!(rng.reentries, 2);
}

#[cfg(feature = "graph")]
#[test]
fn edge_family_sampling_restores_original_edge_ids() {
    use zdd_family::GraphSpace;

    let graph = zdd_family::Graph::from_edges(4, [(0, 1), (1, 2), (2, 3)]).unwrap();
    let space = GraphSpace::new(&graph).unwrap();
    let family = space.matchings().unwrap();
    let mut rng = BytesRng::new([vec![0]]);
    let sample = family.sample(&mut rng).unwrap().unwrap();

    assert!(family.contains(sample.as_slice()).unwrap());
}
