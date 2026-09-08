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
}
