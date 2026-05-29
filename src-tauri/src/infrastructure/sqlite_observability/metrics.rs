use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageGrowthMetrics {
    pub current_page_count: u64,
    pub previous_page_count: u64,
    pub growth: i64,
    pub growth_pct: f64,
    pub current_freelist_count: u64,
    pub previous_freelist_count: u64,
    pub freelist_growth: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionMetrics {
    pub cache_spills: u64,
    pub soft_heap_limit: u64,
    pub page_cache_hit_ratio: Option<f64>,
    pub cache_miss_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryMetrics {
    pub heap_used: u64,
    pub page_cache_used: u64,
    pub lookaside_slots: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregateMetrics {
    pub page_growth: Option<PageGrowthMetrics>,
    pub connection: Option<ConnectionMetrics>,
    pub memory: Option<MemoryMetrics>,
}

impl PageGrowthMetrics {
    pub fn new(
        current_page_count: u64,
        previous_page_count: u64,
        current_freelist_count: u64,
        previous_freelist_count: u64,
    ) -> Self {
        let growth = current_page_count as i64 - previous_page_count as i64;
        let growth_pct = if previous_page_count == 0 {
            0.0
        } else {
            (growth as f64 / previous_page_count as f64) * 100.0
        };
        let freelist_growth = current_freelist_count as i64 - previous_freelist_count as i64;
        Self {
            current_page_count,
            previous_page_count,
            growth,
            growth_pct,
            current_freelist_count,
            previous_freelist_count,
            freelist_growth,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_growth_positive() {
        let metrics = PageGrowthMetrics::new(200, 100, 10, 5);
        assert_eq!(metrics.growth, 100);
        assert!((metrics.growth_pct - 100.0).abs() < f64::EPSILON);
        assert_eq!(metrics.freelist_growth, 5);
    }

    #[test]
    fn page_growth_zero_previous() {
        let metrics = PageGrowthMetrics::new(100, 0, 5, 0);
        assert_eq!(metrics.growth, 100);
        assert_eq!(metrics.growth_pct, 0.0);
    }

    #[test]
    fn page_growth_negative() {
        let metrics = PageGrowthMetrics::new(50, 100, 2, 5);
        assert_eq!(metrics.growth, -50);
        assert!((metrics.growth_pct - (-50.0)).abs() < f64::EPSILON);
        assert_eq!(metrics.freelist_growth, -3);
    }

    #[test]
    fn serde_round_trip() {
        let metrics = PageGrowthMetrics::new(200, 100, 10, 5);
        let json = serde_json::to_string(&metrics).unwrap();
        let deserialized: PageGrowthMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(metrics, deserialized);
    }
}
