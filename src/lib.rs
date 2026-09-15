//! `zdd-family` is a library for representing set families with
//! zero-suppressed decision diagrams (ZDDs).
//!
//! Start with [`FamilySpace`] to create immutable [`SetFamily`] values.

#![forbid(unsafe_code)]

mod family;
mod zdd;

pub use family::{Error, FamilySpace, FamilySpaceBuilder, LimitKind, Limits, SetFamily, VariableId};

#[cfg(feature = "graph")]
mod frontier;
#[cfg(feature = "graph")]
mod graph;
#[cfg(feature = "graph")]
mod problems;
