//! `zdd-family` is a library for representing set families with
//! zero-suppressed decision diagrams (ZDDs).
//!
//! Start with [`FamilySpace`] to create immutable [`SetFamily`] values.

#![forbid(unsafe_code)]

#[cfg(not(target_pointer_width = "64"))]
compile_error!("zdd-family currently supports only 64-bit targets");

mod family;
mod zdd;

#[cfg(test)]
mod test_support;

pub use family::{
    CardinalityFilter, CountError, CountIndex, Error, FamilySpace, FamilySpaceBuilder, LimitKind,
    Limits, OperationReport, OperationStats, QueryError, QueryLimits, QueryStats, SetFamily,
    SpaceStats, VariableId,
};
pub use num_bigint::BigUint;

#[cfg(feature = "graph")]
mod frontier;
#[cfg(feature = "graph")]
mod graph;
#[cfg(feature = "graph")]
mod problems;
