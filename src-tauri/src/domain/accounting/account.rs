use serde::{Deserialize, Serialize};

/// Accounting classification of a stock-affecting event.
///
/// Each variant maps to a specific accounting treatment:
///
/// | Variant       | Accounting Effect                     | Example                      |
/// |---------------|---------------------------------------|------------------------------|
/// | `Order`       | Increases stock, sets cost basis      | Supplier order receipt       |
/// | `Opening`     | Establishes initial stock for a year  | Fiscal year carry-forward    |
/// | `Consumption` | Decreases stock, allocates FIFO cost  | Meal preparation             |
/// | `Adjustment`  | Corrects stock without consumption    | Physical inventory count     |
/// | `Correction`  | Reverses a previous erroneous entry   | Error correction             |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Account {
    Order,
    Opening,
    Consumption,
    Adjustment,
    Correction,
}
