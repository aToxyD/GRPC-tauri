use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WalState {
    Ok,
    CheckpointRequired,
    Overflow,
    IoError,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WalMetrics {
    pub wal_file_size_bytes: u64,
    pub page_count: u64,
    pub freelist_count: u64,
    pub checkpoint_seqno: u64,
    pub wal_state: WalState,
    pub auto_checkpoint_pages: u64,
    pub is_eligible_for_checkpoint: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageCountSnapshot {
    pub page_count: u64,
    pub freelist_count: u64,
    pub page_size: u64,
    pub total_pages: u64,
    pub usable_pages: u64,
}

impl WalMetrics {
    pub fn new(
        wal_file_size_bytes: u64,
        page_count: u64,
        freelist_count: u64,
        checkpoint_seqno: u64,
        wal_state: WalState,
        auto_checkpoint_pages: u64,
    ) -> Self {
        let is_eligible_for_checkpoint = match wal_state {
            WalState::CheckpointRequired | WalState::Overflow => true,
            WalState::Ok | WalState::IoError => false,
        };
        Self {
            wal_file_size_bytes,
            page_count,
            freelist_count,
            checkpoint_seqno,
            wal_state,
            auto_checkpoint_pages,
            is_eligible_for_checkpoint,
        }
    }

    pub fn from_pragma_values(
        wal_file_size_bytes: u64,
        page_count: u64,
        freelist_count: u64,
        checkpoint_seqno: u64,
        wal_mode: i32,
        auto_checkpoint: u64,
    ) -> Self {
        let wal_state = match (wal_mode, checkpoint_seqno) {
            (1, 0) => WalState::Ok,
            (1, 1) => WalState::CheckpointRequired,
            (_, s) if s > 1 => WalState::Overflow,
            _ => WalState::Ok,
        };
        Self::new(
            wal_file_size_bytes,
            page_count,
            freelist_count,
            checkpoint_seqno,
            wal_state,
            auto_checkpoint,
        )
    }

    pub fn wal_page_count(&self) -> u64 {
        if self.page_size() == 0 {
            return 0;
        }
        self.wal_file_size_bytes / self.page_size()
    }

    fn page_size(&self) -> u64 {
        4096
    }

    pub fn estimated_wal_growth_ratio(&self) -> f64 {
        if self.page_count == 0 {
            return 0.0;
        }
        self.wal_page_count() as f64 / self.page_count as f64
    }
}

impl PageCountSnapshot {
    pub fn new(page_count: u64, freelist_count: u64, page_size: u64) -> Self {
        let total_pages = page_count + freelist_count;
        let usable_pages = page_count;
        Self {
            page_count,
            freelist_count,
            page_size,
            total_pages,
            usable_pages,
        }
    }

    pub fn growth_from(&self, earlier: &PageCountSnapshot) -> i64 {
        self.page_count as i64 - earlier.page_count as i64
    }

    pub fn freelist_growth_from(&self, earlier: &PageCountSnapshot) -> i64 {
        self.freelist_count as i64 - earlier.freelist_count as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wal_metrics_construction_ok() {
        let metrics = WalMetrics::new(4096, 100, 5, 0, WalState::Ok, 1000);
        assert!(!metrics.is_eligible_for_checkpoint);
        assert_eq!(metrics.wal_page_count(), 1);
    }

    #[test]
    fn wal_metrics_checkpoint_eligible() {
        let metrics = WalMetrics::new(65536, 100, 5, 1, WalState::CheckpointRequired, 1000);
        assert!(metrics.is_eligible_for_checkpoint);
    }

    #[test]
    fn wal_metrics_overflow_eligible() {
        let metrics = WalMetrics::new(65536, 100, 5, 5, WalState::Overflow, 1000);
        assert!(metrics.is_eligible_for_checkpoint);
    }

    #[test]
    fn from_pragma_values_ok() {
        let metrics = WalMetrics::from_pragma_values(4096, 100, 5, 0, 1, 1000);
        assert_eq!(metrics.wal_state, WalState::Ok);
    }

    #[test]
    fn from_pragma_values_checkpoint_required() {
        let metrics = WalMetrics::from_pragma_values(65536, 100, 5, 1, 1, 1000);
        assert_eq!(metrics.wal_state, WalState::CheckpointRequired);
    }

    #[test]
    fn from_pragma_values_overflow() {
        let metrics = WalMetrics::from_pragma_values(131072, 100, 5, 3, 1, 1000);
        assert_eq!(metrics.wal_state, WalState::Overflow);
    }

    #[test]
    fn page_count_snapshot_growth() {
        let earlier = PageCountSnapshot::new(100, 5, 4096);
        let later = PageCountSnapshot::new(150, 8, 4096);
        assert_eq!(later.growth_from(&earlier), 50);
        assert_eq!(later.freelist_growth_from(&earlier), 3);
    }

    #[test]
    fn serde_round_trip_wal_metrics() {
        let metrics = WalMetrics::new(4096, 100, 5, 0, WalState::Ok, 1000);
        let json = serde_json::to_string(&metrics).unwrap();
        let deserialized: WalMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(metrics, deserialized);
    }

    #[test]
    fn serde_round_trip_page_count() {
        let snap = PageCountSnapshot::new(100, 5, 4096);
        let json = serde_json::to_string(&snap).unwrap();
        let deserialized: PageCountSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap, deserialized);
    }

    #[test]
    fn estimated_growth_ratio() {
        let metrics = WalMetrics::new(4096 * 50, 100, 5, 0, WalState::Ok, 1000);
        let ratio = metrics.estimated_wal_growth_ratio();
        assert!((ratio - 0.5).abs() < f64::EPSILON);
    }
}
