//! Internal graph-problem implementations.

mod cycles;
mod matchings;
mod paths;

#[cfg(test)]
#[path = "../../tests/internal/frontier.rs"]
mod tests;

pub(crate) use cycles::CycleProblem;
pub(crate) use matchings::MatchingProblem;
pub(crate) use paths::PathProblem;
