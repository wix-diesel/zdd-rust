//! Internal graph-problem implementations.

mod cycles;
mod matchings;
mod paths;

pub(crate) use cycles::CycleProblem;
pub(crate) use matchings::MatchingProblem;
pub(crate) use paths::PathProblem;
