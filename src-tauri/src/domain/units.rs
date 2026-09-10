//! Canonical unit-of-measure and TVA classification value types (SEC-087).
//!
//! These are the ten measurement units of the GRPC catalog. They are DISTINCT
//! from the organizational `units` table (Protection Civile units): measurement
//! units are domain value types persisted as INTEGER codes `1..=10`, while
//! organizational units are UUID-keyed rows.
//!
//! Codes are stable and MUST NOT be renumbered once emitted into persisted
//! state (contract snapshots, order items, FIFO layers).

use std::fmt;

use crate::domain::numeric::{Money, NumericError, Quantity, Rate};

/// Canonical measurement units (SEC-087). Codes `1..=10` are persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnitMeasure {
    /// كيلوغرام — Kilogramme.
    Kilogram = 1,
    /// لتر — Litre.
    Liter = 2,
    /// دلو — Seau.
    Bucket = 3,
    /// قارورة — Bouteille.
    Bottle = 4,
    /// صفيحة — Plateau (tray).
    Tray = 5,
    /// قطعة — Pièce.
    Piece = 6,
    /// بيضة — Œuf.
    Egg = 7,
    /// علبة — Boîte.
    Box = 8,
    /// كيس — Sac.
    Bag = 9,
    /// خبزة — Pain (loaf).
    Loaf = 10,
}

impl UnitMeasure {
    /// All units in canonical (code) order.
    pub const ALL: [UnitMeasure; 10] = [
        UnitMeasure::Kilogram,
        UnitMeasure::Liter,
        UnitMeasure::Bucket,
        UnitMeasure::Bottle,
        UnitMeasure::Tray,
        UnitMeasure::Piece,
        UnitMeasure::Egg,
        UnitMeasure::Box,
        UnitMeasure::Bag,
        UnitMeasure::Loaf,
    ];

    /// Stable persisted integer code (`1..=10`).
    pub fn code(self) -> i32 {
        self as i32
    }

    /// Canonical French label.
    pub fn label_fr(self) -> &'static str {
        match self {
            UnitMeasure::Kilogram => "kg",
            UnitMeasure::Liter => "L",
            UnitMeasure::Bucket => "Seau",
            UnitMeasure::Bottle => "Bouteille",
            UnitMeasure::Tray => "Plateau",
            UnitMeasure::Piece => "Pièce",
            UnitMeasure::Egg => "Œuf",
            UnitMeasure::Box => "Boîte",
            UnitMeasure::Bag => "Sac",
            UnitMeasure::Loaf => "Pain",
        }
    }

    /// Canonical Arabic label.
    pub fn label_ar(self) -> &'static str {
        match self {
            UnitMeasure::Kilogram => "كلغ",
            UnitMeasure::Liter => "لتر",
            UnitMeasure::Bucket => "دلو",
            UnitMeasure::Bottle => "قارورة",
            UnitMeasure::Tray => "صفيحة",
            UnitMeasure::Piece => "قطعة",
            UnitMeasure::Egg => "بيضة",
            UnitMeasure::Box => "علبة",
            UnitMeasure::Bag => "كيس",
            UnitMeasure::Loaf => "خبزة",
        }
    }
}

/// Error raised when an integer cannot decode to a [`UnitMeasure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownUnitCode(pub i32);

impl fmt::Display for UnknownUnitCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown unit-measure code {}", self.0)
    }
}

impl std::error::Error for UnknownUnitCode {}

impl TryFrom<i32> for UnitMeasure {
    type Error = UnknownUnitCode;

    fn try_from(code: i32) -> Result<Self, Self::Error> {
        match code {
            1 => Ok(UnitMeasure::Kilogram),
            2 => Ok(UnitMeasure::Liter),
            3 => Ok(UnitMeasure::Bucket),
            4 => Ok(UnitMeasure::Bottle),
            5 => Ok(UnitMeasure::Tray),
            6 => Ok(UnitMeasure::Piece),
            7 => Ok(UnitMeasure::Egg),
            8 => Ok(UnitMeasure::Box),
            9 => Ok(UnitMeasure::Bag),
            10 => Ok(UnitMeasure::Loaf),
            _ => Err(UnknownUnitCode(code)),
        }
    }
}

/// Product TVA classification (SEC-087). Codes `0..=2` are persisted.
///
/// Exactly three classes: EXONÉRÉ, 9 %, and 19 %. Rates use the monetary
/// scale-4 convention (19 % == `190_000` permyriad-of-percent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TvaClassification {
    /// Exonéré (0 %).
    Exonere = 0,
    /// Taux réduit 9 %.
    NinePercent = 1,
    /// Taux standard 19 %.
    NineteenPercent = 2,
}

impl TvaClassification {
    /// All classes in canonical (code) order.
    pub const ALL: [TvaClassification; 3] = [
        TvaClassification::Exonere,
        TvaClassification::NinePercent,
        TvaClassification::NineteenPercent,
    ];

    /// Stable persisted integer code (`0..=2`).
    pub fn code(self) -> i32 {
        self as i32
    }

    /// Exact percent-domain [`Rate`] for this class. The **only** place the
    /// monetary rate-permyriad is mapped to an arithmetic rate (P2 / single
    /// source of truth): contract pricing always sources its rate from the
    /// product's classification, never from the legacy fiscal-year policy.
    pub fn rate(self) -> Rate {
        Rate::from_scaled_i64(self.rate_permyriad())
            .expect("TVA class rates are the closed set 0/90_000/190_000 ≤ 100%")
    }

    /// Rate in permyriad-of-percent on the monetary scale-4 convention
    /// (19 % -> `190_000`).
    pub fn rate_permyriad(self) -> i64 {
        match self {
            TvaClassification::Exonere => 0,
            TvaClassification::NinePercent => 90_000,
            TvaClassification::NineteenPercent => 190_000,
        }
    }

    /// Canonical French label.
    pub fn label(self) -> &'static str {
        match self {
            TvaClassification::Exonere => "EXONÉRÉ",
            TvaClassification::NinePercent => "9 %",
            TvaClassification::NineteenPercent => "19 %",
        }
    }
}

/// Error raised when an integer cannot decode to a [`TvaClassification`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownTvaClassification(pub i32);

impl fmt::Display for UnknownTvaClassification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown TVA classification code {}", self.0)
    }
}

impl std::error::Error for UnknownTvaClassification {}

impl TryFrom<i32> for TvaClassification {
    type Error = UnknownTvaClassification;

    fn try_from(code: i32) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(TvaClassification::Exonere),
            1 => Ok(TvaClassification::NinePercent),
            2 => Ok(TvaClassification::NineteenPercent),
            _ => Err(UnknownTvaClassification(code)),
        }
    }
}

/// A validated product unit/TVA configuration (SEC-087): closed-set unit codes
/// plus the positive integer purchase→consumption conversion factor. Produced
/// **only** by `domain::validation::validate_product_units`; never hand-built by
/// callers, so an invalid combination cannot reach a write path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductUnitConfig {
    /// Unit in which HT price and contract quantities are expressed.
    pub purchase_unit: UnitMeasure,
    /// Unit in which consumption/inventory is expressed.
    pub consumption_unit: UnitMeasure,
    /// Positive integer purchase→consumption factor; must be `1` when the two
    /// units are identical.
    pub conversion_factor: i32,
    /// Current classification; its `rate()` sources all contract TVA math.
    pub tva_classification: TvaClassification,
}

/// Error raised when persisted unit-snapshot codes cannot form a valid
/// [`OrderUnitSnapshot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitSnapshotError {
    /// Only part of an order-item's purchase→consumption snapshot is present.
    Partial,
    /// The purchase-unit code does not decode into the closed `1..=10` set.
    UnknownPurchaseUnit(UnknownUnitCode),
    /// The consumption-unit code does not decode into the closed `1..=10` set.
    UnknownConsumptionUnit(UnknownUnitCode),
    /// Same-unit snapshots must use factor `1`; differing units require a
    /// strictly positive factor (mirrors `contract_products` CHECK).
    InvalidFactor,
}

impl fmt::Display for UnitSnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnitSnapshotError::Partial => {
                write!(
                    f,
                    "غير مكتمل — يجب أن تُحفظ بيانات الوحدات كاملة أو تُترك فارغة"
                )
            }
            UnitSnapshotError::UnknownPurchaseUnit(code) => {
                write!(f, "وحدة الشراء غير معروفة: {}", code.0)
            }
            UnitSnapshotError::UnknownConsumptionUnit(code) => {
                write!(f, "وحدة الاستهلاك غير معروفة: {}", code.0)
            }
            UnitSnapshotError::InvalidFactor => {
                write!(f, "معامل التحويل غير صالح (نفس الوحدة ⇒ 1، مختلفة ⇒ > 0)")
            }
        }
    }
}

impl std::error::Error for UnitSnapshotError {}

/// Immutable purchase→consumption unit snapshot of a supplier-order item
/// (SEC-087 Phase 5).
///
/// Captured ONCE at order creation from the resolved contract product and
/// persisted on `supplier_order_items`. The receipt path converts using this
/// snapshot only — never a re-derived unit, so an order keeps its agreed
/// purchase semantics even if the contract later changes. A fully-NULL
/// snapshot denotes a legacy pre-Phase-5 order item and converts 1:1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderUnitSnapshot {
    /// Unit in which `supplier_order_items.quantity` and `price_ttc` are
    /// expressed (`None` for legacy items).
    pub purchase_unit: Option<UnitMeasure>,
    /// Unit in which FIFO layers and inventory stock are expressed.
    pub consumption_unit: Option<UnitMeasure>,
    /// Integer purchase→consumption factor; legacy items default to `1`.
    pub conversion_factor: Option<i32>,
}

impl OrderUnitSnapshot {
    /// Build from persisted wire codes. The three parts are all-or-nothing: a
    /// partial snapshot is corrupt and fails closed. Operationals codes inside
    /// `1..=10` decode strictly; the unit-consistency CHECK mirrors the
    /// `contract_products` table constraint (same unit ⇒ factor `1`, different
    /// ⇒ factor `> 0`).
    pub fn from_codes(
        purchase_unit: Option<i32>,
        consumption_unit: Option<i32>,
        conversion_factor: Option<i32>,
    ) -> Result<Self, UnitSnapshotError> {
        let present = [
            purchase_unit.is_some(),
            consumption_unit.is_some(),
            conversion_factor.is_some(),
        ]
        .iter()
        .filter(|present| **present)
        .count();
        if present != 0 && present != 3 {
            return Err(UnitSnapshotError::Partial);
        }
        let purchase_unit = match purchase_unit {
            Some(code) => {
                Some(UnitMeasure::try_from(code).map_err(UnitSnapshotError::UnknownPurchaseUnit)?)
            }
            None => None,
        };
        let consumption_unit = match consumption_unit {
            Some(code) => Some(
                UnitMeasure::try_from(code).map_err(UnitSnapshotError::UnknownConsumptionUnit)?,
            ),
            None => None,
        };
        if let (Some(purchase), Some(consumption), Some(factor)) =
            (purchase_unit, consumption_unit, conversion_factor)
        {
            if purchase == consumption && factor != 1 {
                return Err(UnitSnapshotError::InvalidFactor);
            }
            if purchase != consumption && factor <= 0 {
                return Err(UnitSnapshotError::InvalidFactor);
            }
        }
        Ok(Self {
            purchase_unit,
            consumption_unit,
            conversion_factor,
        })
    }

    /// `true` for legacy pre-Phase-5 order items (no persisted snapshot).
    pub fn is_legacy(&self) -> bool {
        self.purchase_unit.is_none()
    }

    /// Effective purchase→consumption factor; legacy items convert 1:1.
    pub fn effective_factor(&self) -> i32 {
        self.conversion_factor.unwrap_or(1)
    }
}

/// Exact purchase→consumption conversion of ONE receipt line (SEC-087 Phase 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceiptConversion {
    /// `purchase_quantity × factor`, exact `Quantity` (scale-3).
    pub consumption_quantity: Quantity,
    /// `price_ttc / factor` at full decimal precision. Rounded to the cent ONCE
    /// (Option A) only at the persistence boundary via `to_scaled_i64`; no
    /// running remainder is carried across lines or receipts.
    pub unit_cost_consumption_unit: Money,
}

impl OrderUnitSnapshot {
    /// Convert one order item at receipt time.
    ///
    /// Deterministic and exact (ADR-0048): the consumption quantity is the
    /// exact purchase quantity scaled by the integer factor, and the FIFO unit
    /// cost is `price_ttc ÷ factor` keeping full `Decimal` precision — the
    /// single scale-2 rounding happens only when the layer is written. No
    /// running remainder is carried. The bounded residual is at most half a
    /// cent per consumption unit, i.e. `≤ 0.005 × consumption_quantity`
    /// (e.g. 7.00 DA / 3 → 2.33 DA × 3 = 6.99 DA, residual 0.01 ≤ 0.015);
    /// this rounding is accepted and documented for Phase 5.
    pub fn to_receipt(
        &self,
        unit_price: Money,
        purchase_quantity: Quantity,
    ) -> Result<ReceiptConversion, NumericError> {
        let factor = self.effective_factor();
        let consumption_quantity = purchase_quantity.checked_mul_scalar(factor as i64)?;
        let unit_cost_consumption_unit = unit_price.checked_div_scalar(factor as i64)?;
        Ok(ReceiptConversion {
            consumption_quantity,
            unit_cost_consumption_unit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_measures_cover_1_to_10_fail_closed() {
        for (index, expected) in UnitMeasure::ALL.iter().enumerate() {
            let code = (index as i32) + 1;
            assert_eq!(UnitMeasure::try_from(code), Ok(*expected));
            assert_eq!(expected.code(), code);
        }
        for code in [-1, 0, 11, 100] {
            assert!(UnitMeasure::try_from(code).is_err());
        }
    }

    #[test]
    fn unit_measure_labels_are_stable() {
        assert_eq!(UnitMeasure::Kilogram.label_fr(), "kg");
        assert_eq!(UnitMeasure::Kilogram.label_ar(), "كلغ");
        assert_eq!(UnitMeasure::Tray.label_fr(), "Plateau");
        assert_eq!(UnitMeasure::Tray.label_ar(), "صفيحة");
        assert_eq!(UnitMeasure::Loaf.label_fr(), "Pain");
        assert_eq!(UnitMeasure::Loaf.label_ar(), "خبزة");
    }

    #[test]
    fn tva_classifications_cover_0_to_2_fail_closed() {
        for (index, expected) in TvaClassification::ALL.iter().enumerate() {
            let code = index as i32;
            assert_eq!(TvaClassification::try_from(code), Ok(*expected));
            assert_eq!(expected.code(), code);
        }
        for code in [-1, 3, 100] {
            assert!(TvaClassification::try_from(code).is_err());
        }
    }

    #[test]
    fn tva_rates_follow_scale4_convention() {
        assert_eq!(TvaClassification::Exonere.rate_permyriad(), 0);
        assert_eq!(TvaClassification::NinePercent.rate_permyriad(), 90_000);
        assert_eq!(TvaClassification::NineteenPercent.rate_permyriad(), 190_000);
        assert_eq!(TvaClassification::Exonere.label(), "EXONÉRÉ");
        assert_eq!(TvaClassification::NineteenPercent.label(), "19 %");
    }

    #[test]
    fn tva_classification_rates_are_percent_domain() {
        assert_eq!(
            TvaClassification::Exonere.rate(),
            Rate::parse_str("0").unwrap()
        );
        assert_eq!(
            TvaClassification::NinePercent.rate(),
            Rate::parse_str("9").unwrap()
        );
        assert_eq!(
            TvaClassification::NineteenPercent.rate(),
            Rate::parse_str("19").unwrap()
        );
        assert_eq!(
            TvaClassification::NineteenPercent
                .rate()
                .to_scaled_i64()
                .unwrap(),
            190_000
        );
    }

    #[test]
    fn product_unit_config_is_the_validated_value_set() {
        let config = crate::domain::validation::validate_product_units(
            Some(UnitMeasure::Kilogram.code()),
            Some(UnitMeasure::Kilogram.code()),
            Some(1),
            Some(TvaClassification::NineteenPercent.code()),
        )
        .unwrap();
        assert_eq!(config.purchase_unit, UnitMeasure::Kilogram);
        assert_eq!(config.consumption_unit, UnitMeasure::Kilogram);
        assert_eq!(config.conversion_factor, 1);
        assert_eq!(
            config.tva_classification,
            TvaClassification::NineteenPercent
        );
    }

    #[test]
    fn snapshot_parts_are_all_or_nothing() {
        assert!(OrderUnitSnapshot::from_codes(Some(1), Some(1), Some(1)).is_ok());
        assert!(OrderUnitSnapshot::from_codes(None, None, None).is_ok());
        assert_eq!(
            OrderUnitSnapshot::from_codes(Some(1), Some(1), None),
            Err(UnitSnapshotError::Partial)
        );
        assert_eq!(
            OrderUnitSnapshot::from_codes(Some(1), None, Some(1)),
            Err(UnitSnapshotError::Partial)
        );
        assert_eq!(
            OrderUnitSnapshot::from_codes(None, Some(1), Some(1)),
            Err(UnitSnapshotError::Partial)
        );
    }

    #[test]
    fn snapshot_decodes_codes_fail_closed() {
        let snap = OrderUnitSnapshot::from_codes(Some(8), Some(1), Some(10)).unwrap();
        assert_eq!(snap.purchase_unit, Some(UnitMeasure::Box));
        assert_eq!(snap.consumption_unit, Some(UnitMeasure::Kilogram));
        assert_eq!(snap.conversion_factor, Some(10));
        assert!(matches!(
            OrderUnitSnapshot::from_codes(Some(99), Some(1), Some(1)),
            Err(UnitSnapshotError::UnknownPurchaseUnit(_))
        ));
        assert!(matches!(
            OrderUnitSnapshot::from_codes(Some(1), Some(0), Some(1)),
            Err(UnitSnapshotError::UnknownConsumptionUnit(_))
        ));
        assert!(OrderUnitSnapshot::from_codes(Some(1), Some(1), Some(1)).is_ok());
    }

    #[test]
    fn snapshot_enforces_contract_products_factor_check() {
        // Same unit must convert 1:1.
        assert_eq!(
            OrderUnitSnapshot::from_codes(Some(1), Some(1), Some(2)),
            Err(UnitSnapshotError::InvalidFactor)
        );
        // Different units require a strictly positive factor.
        assert!(OrderUnitSnapshot::from_codes(Some(8), Some(1), Some(10)).is_ok());
        assert_eq!(
            OrderUnitSnapshot::from_codes(Some(8), Some(1), Some(0)),
            Err(UnitSnapshotError::InvalidFactor)
        );
    }

    #[test]
    fn snapshot_legacy_converts_11() {
        let legacy = OrderUnitSnapshot::from_codes(None, None, None).unwrap();
        assert!(legacy.is_legacy());
        assert_eq!(legacy.effective_factor(), 1);
    }

    #[test]
    fn same_unit_receipt_is_identity() {
        let snap = OrderUnitSnapshot::from_codes(Some(1), Some(1), Some(1)).unwrap();
        let receipt = snap
            .to_receipt(
                Money::parse_str("40.00").unwrap(),
                Quantity::parse_str("3.500").unwrap(),
            )
            .unwrap();
        assert_eq!(
            receipt.consumption_quantity,
            Quantity::parse_str("3.500").unwrap()
        );
        assert_eq!(
            receipt.unit_cost_consumption_unit,
            Money::parse_str("40.00").unwrap()
        );
    }

    #[test]
    fn integer_factor_conversion_is_exact() {
        // 1 box (factor 10) at 120.00 DA/box → 10.000 kg, 12.00 DA/kg.
        let snap = OrderUnitSnapshot::from_codes(Some(8), Some(1), Some(10)).unwrap();
        let receipt = snap
            .to_receipt(
                Money::parse_str("120.00").unwrap(),
                Quantity::parse_str("1.000").unwrap(),
            )
            .unwrap();
        assert_eq!(
            receipt.consumption_quantity,
            Quantity::parse_str("10.000").unwrap()
        );
        assert_eq!(
            receipt.unit_cost_consumption_unit,
            Money::parse_str("12.00").unwrap()
        );
        assert_eq!(
            receipt.unit_cost_consumption_unit.to_scaled_i64().unwrap(),
            1200
        );
    }

    #[test]
    fn non_terminating_conversion_rounds_once_to_cent() {
        // 7.00 / 3 keeps full precision internally; the scale-2 boundary rounds
        // exactly once: 2.333... → 2.33 (233 centimes).
        let snap = OrderUnitSnapshot::from_codes(Some(6), Some(1), Some(3)).unwrap();
        let receipt = snap
            .to_receipt(
                Money::parse_str("7.00").unwrap(),
                Quantity::parse_str("1.000").unwrap(),
            )
            .unwrap();
        assert_eq!(
            receipt.consumption_quantity,
            Quantity::parse_str("3.000").unwrap()
        );
        assert_eq!(
            receipt.unit_cost_consumption_unit.to_scaled_i64().unwrap(),
            233
        );
        // Consumption valuation 3 × 2.33 = 6.99 vs purchase value 7.00;
        // residual 0.01 ≤ 0.005 × consumption qty (3.000) = 0.015.
        let valued = Money::from_centimes(233)
            .unwrap()
            .checked_mul_scalar(3)
            .unwrap();
        assert_eq!(valued.to_scaled_i64().unwrap(), 699);
        let exact_purchase = Money::parse_str("7.00").unwrap();
        let residual = exact_purchase.checked_sub(valued).unwrap();
        let bound = Money::parse_str("0.005")
            .unwrap()
            .checked_mul_quantity(&receipt.consumption_quantity)
            .unwrap();
        assert!(residual <= bound, "residual {residual} exceeds {bound}");
    }
}
