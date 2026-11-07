//! Score statistics and source similarity detection.

pub mod similarity;
pub mod stats;

pub use stats::{compute, parse_groups, ContestStats, Groups};
