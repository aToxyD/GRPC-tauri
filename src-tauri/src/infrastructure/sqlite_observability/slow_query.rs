use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlowQueryThreshold {
    Milliseconds(u64),
}

impl Default for SlowQueryThreshold {
    fn default() -> Self {
        Self::Milliseconds(100)
    }
}

impl SlowQueryThreshold {
    pub fn as_millis(&self) -> u64 {
        match self {
            SlowQueryThreshold::Milliseconds(ms) => *ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryExecutionSample {
    pub sql_hash: u64,
    pub sql: String,
    pub duration_ms: u64,
    pub rows_affected: u64,
    pub category: QueryCategory,
    pub recorded_at_order: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryCategory {
    Select,
    Insert,
    Update,
    Delete,
    Ddl,
    Pragma,
    Other,
}

fn categorize_sql(sql: &str) -> QueryCategory {
    let trimmed = sql.trim().to_uppercase();
    if trimmed.starts_with("SELECT") {
        QueryCategory::Select
    } else if trimmed.starts_with("INSERT") {
        QueryCategory::Insert
    } else if trimmed.starts_with("UPDATE") {
        QueryCategory::Update
    } else if trimmed.starts_with("DELETE") {
        QueryCategory::Delete
    } else if trimmed.starts_with("PRAGMA") {
        QueryCategory::Pragma
    } else if trimmed.starts_with("CREATE")
        || trimmed.starts_with("ALTER")
        || trimmed.starts_with("DROP")
    {
        QueryCategory::Ddl
    } else {
        QueryCategory::Other
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowQueryRecord {
    threshold: SlowQueryThreshold,
    samples: VecDeque<QueryExecutionSample>,
    max_samples: usize,
    counter: u64,
}

impl SlowQueryRecord {
    pub fn new(max_samples: usize) -> Self {
        Self {
            threshold: SlowQueryThreshold::default(),
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
            counter: 0,
        }
    }

    pub fn with_threshold(max_samples: usize, threshold: SlowQueryThreshold) -> Self {
        Self {
            threshold,
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
            counter: 0,
        }
    }

    pub fn record(&mut self, sql: &str, duration_ms: u64, rows_affected: u64) {
        if duration_ms < self.threshold.as_millis() {
            return;
        }
        self.counter += 1;
        let sample = QueryExecutionSample {
            sql_hash: hash_sql(sql),
            sql: sql.to_string(),
            duration_ms,
            rows_affected,
            category: categorize_sql(sql),
            recorded_at_order: self.counter,
        };
        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn samples(&self) -> Vec<QueryExecutionSample> {
        let mut v: Vec<_> = self.samples.iter().cloned().collect();
        v.sort_by_key(|s| s.recorded_at_order);
        v
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn threshold(&self) -> SlowQueryThreshold {
        self.threshold
    }

    pub fn slowest(&self) -> Option<QueryExecutionSample> {
        self.samples.iter().max_by_key(|s| s.duration_ms).cloned()
    }

    pub fn by_category(&self, category: QueryCategory) -> Vec<QueryExecutionSample> {
        let mut v: Vec<_> = self
            .samples
            .iter()
            .filter(|s| s.category == category)
            .cloned()
            .collect();
        v.sort_by_key(|s| s.recorded_at_order);
        v
    }

    pub fn average_duration_ms(&self) -> f64 {
        let count = self.samples.len();
        if count == 0 {
            return 0.0;
        }
        let total: u64 = self.samples.iter().map(|s| s.duration_ms).sum();
        total as f64 / count as f64
    }
}

fn hash_sql(sql: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    sql.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_record_below_threshold() {
        let mut record = SlowQueryRecord::new(100);
        record.record("SELECT 1", 50, 0);
        assert!(record.is_empty());
    }

    #[test]
    fn records_above_threshold() {
        let mut record = SlowQueryRecord::new(100);
        record.record("SELECT * FROM products", 150, 100);
        assert_eq!(record.len(), 1);
    }

    #[test]
    fn records_with_custom_threshold() {
        let mut record =
            SlowQueryRecord::with_threshold(100, SlowQueryThreshold::Milliseconds(200));
        record.record("SELECT 1", 150, 0);
        assert!(record.is_empty());
        record.record("SELECT 1", 250, 0);
        assert_eq!(record.len(), 1);
    }

    #[test]
    fn bounded_history_eviction() {
        let mut record = SlowQueryRecord::new(3);
        for i in 0..10 {
            record.record(&format!("SELECT {}", i), 200, 0);
        }
        assert_eq!(record.len(), 3);
    }

    #[test]
    fn samples_are_deterministically_ordered() {
        let mut record = SlowQueryRecord::new(10);
        record.record("SELECT c", 200, 0);
        record.record("SELECT a", 200, 0);
        record.record("SELECT b", 200, 0);
        let samples = record.samples();
        assert_eq!(samples[0].sql, "SELECT c");
        assert_eq!(samples[1].sql, "SELECT a");
        assert_eq!(samples[2].sql, "SELECT b");
    }

    #[test]
    fn categorizes_queries() {
        let mut record = SlowQueryRecord::new(10);
        record.record("SELECT * FROM t", 200, 0);
        record.record("INSERT INTO t VALUES (1)", 200, 0);
        record.record("UPDATE t SET x = 1", 200, 0);
        record.record("DELETE FROM t", 200, 0);
        record.record("PRAGMA page_count", 200, 0);
        record.record("CREATE TABLE t (id INT)", 200, 0);

        assert_eq!(record.by_category(QueryCategory::Select).len(), 1);
        assert_eq!(record.by_category(QueryCategory::Insert).len(), 1);
        assert_eq!(record.by_category(QueryCategory::Update).len(), 1);
        assert_eq!(record.by_category(QueryCategory::Delete).len(), 1);
        assert_eq!(record.by_category(QueryCategory::Pragma).len(), 1);
        assert_eq!(record.by_category(QueryCategory::Ddl).len(), 1);
    }

    #[test]
    fn slowest_returns_max_duration() {
        let mut record = SlowQueryRecord::new(10);
        record.record("SELECT 1", 200, 0);
        record.record("SELECT 2", 500, 0);
        record.record("SELECT 3", 100, 0);
        let slowest = record.slowest().unwrap();
        assert_eq!(slowest.duration_ms, 500);
    }

    #[test]
    fn average_duration() {
        let mut record = SlowQueryRecord::new(10);
        record.record("SELECT 1", 100, 0);
        record.record("SELECT 2", 200, 0);
        record.record("SELECT 3", 300, 0);
        assert!((record.average_duration_ms() - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn clear_removes_all() {
        let mut record = SlowQueryRecord::new(10);
        record.record("SELECT 1", 200, 0);
        record.clear();
        assert!(record.is_empty());
    }

    #[test]
    fn serde_round_trip() {
        let mut record = SlowQueryRecord::new(10);
        record.record("SELECT 1", 200, 5);
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: SlowQueryRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record.len(), deserialized.len());
        assert_eq!(record.samples(), deserialized.samples());
    }

    #[test]
    fn threshold_default_is_100ms() {
        let threshold = SlowQueryThreshold::default();
        assert_eq!(threshold.as_millis(), 100);
    }
}
