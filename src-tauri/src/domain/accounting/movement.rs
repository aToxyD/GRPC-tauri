use serde::{Deserialize, Serialize};

/// Direction of a stock movement.
///
/// Determines how the movement affects inventory balance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MovementDirection {
    In,
    Out,
    Opening,
}
