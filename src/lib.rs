//! `zdd-family` is a library for representing set families with
//! zero-suppressed decision diagrams (ZDDs).
//!
//! Start with [`FamilySpace`] to create immutable [`SetFamily`] values.

#![forbid(unsafe_code)]

#[cfg(not(target_pointer_width = "64"))]
compile_error!("zdd-family currently supports only 64-bit targets");

mod family;
mod zdd;

pub use family::{
    Error, FamilySpace, FamilySpaceBuilder, LimitKind, Limits, OperationReport, OperationStats,
    SetFamily, SpaceStats, VariableId,
};

#[cfg(feature = "graph")]
mod frontier;
#[cfg(feature = "graph")]
mod graph;
#[cfg(feature = "graph")]
mod problems;
