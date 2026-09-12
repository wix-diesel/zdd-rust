//! `zdd-family` is a library for representing set families with
//! zero-suppressed decision diagrams (ZDDs).
//!
//! The crate skeleton intentionally exposes no product API yet. Subsequent
//! issues will add the documented API incrementally.

#![forbid(unsafe_code)]

mod family;
mod zdd;

#[cfg(feature = "graph")]
mod frontier;
#[cfg(feature = "graph")]
mod graph;
#[cfg(feature = "graph")]
mod problems;
