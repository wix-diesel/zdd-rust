//! Independent, explicit-set oracle shared by small exhaustive and property tests.
//!
//! This module deliberately uses only the public family API. It does not reuse
//! ZDD nodes, cofactors, apply rules, or backend identifiers.

use std::collections::BTreeSet;

use crate::{FamilySpace, SetFamily};
use proptest::prelude::*;

pub(crate) fn oracle_family_strategy(variable_count: usize) -> BoxedStrategy<OracleFamily> {
    assert!(variable_count <= 6);
    any::<u64>()
        .prop_map(move |mask| OracleFamily::from_family_mask(variable_count, mask))
        .boxed()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OracleFamily {
    variable_count: usize,
    sets: BTreeSet<u64>,
}

impl OracleFamily {
    pub(crate) fn from_family_mask(variable_count: usize, mask: u64) -> Self {
        assert!(
            variable_count <= 6,
            "the compact test oracle supports at most 6 variables"
        );
        let subset_count = 1usize << variable_count;
        let sets = (0..subset_count)
            .filter(|subset| mask & (1u64 << subset) != 0)
            .map(|subset| subset as u64)
            .collect();
        Self {
            variable_count,
            sets,
        }
    }

    pub(crate) fn build_in(&self, space: &FamilySpace) -> SetFamily {
        let sets = self.sets.iter().map(|&set| {
            (0..self.variable_count)
                .filter(|variable| set & (1u64 << variable) != 0)
                .map(|variable| space.variable(variable).unwrap())
                .collect::<Vec<_>>()
        });
        space.from_sets(sets).unwrap()
    }

    /// Builds the same value with reversed elements plus duplicate elements
    /// and solutions, exercising input normalization independently of the ZDD.
    pub(crate) fn build_in_with_normalization_noise(&self, space: &FamilySpace) -> SetFamily {
        let mut sets = Vec::new();
        for &set in self.sets.iter().rev() {
            let mut elements = (0..self.variable_count)
                .rev()
                .filter(|variable| set & (1u64 << variable) != 0)
                .map(|variable| space.variable(variable).unwrap())
                .collect::<Vec<_>>();
            if let Some(&first) = elements.first() {
                elements.push(first);
            }
            sets.push(elements.clone());
            sets.push(elements);
        }
        space.from_sets(sets).unwrap()
    }

    pub(crate) fn union(&self, other: &Self) -> Self {
        self.binary(other, |left, right| left || right)
    }

    pub(crate) fn intersection(&self, other: &Self) -> Self {
        self.binary(other, |left, right| left && right)
    }

    pub(crate) fn difference(&self, other: &Self) -> Self {
        self.binary(other, |left, right| left && !right)
    }

    pub(crate) fn symmetric_difference(&self, other: &Self) -> Self {
        self.binary(other, |left, right| left != right)
    }

    pub(crate) fn is_subset_of(&self, other: &Self) -> bool {
        self.sets.is_subset(&other.sets)
    }

    pub(crate) fn assert_matches(&self, space: &FamilySpace, actual: &SetFamily) {
        for subset in 0..(1usize << self.variable_count) {
            let elements = (0..self.variable_count)
                .rev()
                .filter(|variable| subset & (1usize << variable) != 0)
                .map(|variable| space.variable(variable).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                actual.contains(&elements).unwrap(),
                self.sets.contains(&(subset as u64)),
                "membership differed for subset {subset:0width$b}",
                width = self.variable_count,
            );
        }
        assert_eq!(actual.is_empty(), self.sets.is_empty());
    }

    fn binary(&self, other: &Self, include: impl Fn(bool, bool) -> bool) -> Self {
        assert_eq!(self.variable_count, other.variable_count);
        let subset_count = 1usize << self.variable_count;
        let sets = (0..subset_count as u64)
            .filter(|set| include(self.sets.contains(set), other.sets.contains(set)))
            .collect();
        Self {
            variable_count: self.variable_count,
            sets,
        }
    }
}
