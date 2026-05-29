use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointPolicy {
    pub max_wal_size_bytes: u64,
    pub idle_checkpoint_enabled: bool,
    pub preferred_mode: crate::infrastructure::sqlite_runtime::checkpoint::CheckpointMode,
}

impl Default for CheckpointPolicy {
    fn default() -> Self {
        Self {
            max_wal_size_bytes: 100 * 1024 * 1024,
            idle_checkpoint_enabled: true,
            preferred_mode:
                crate::infrastructure::sqlite_runtime::checkpoint::CheckpointMode::Truncate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy() {
        let policy = CheckpointPolicy::default();
        assert_eq!(policy.max_wal_size_bytes, 100 * 1024 * 1024);
        assert!(policy.idle_checkpoint_enabled);
    }

    #[test]
    fn serde_round_trip() {
        let policy = CheckpointPolicy::default();
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: CheckpointPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, deserialized);
    }
}
