CREATE TABLE IF NOT EXISTS domain_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    transaction_id TEXT NOT NULL,
    sequence_number INTEGER NOT NULL,
    category TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_body TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(transaction_id, sequence_number)
);

CREATE INDEX IF NOT EXISTS idx_domain_events_replay
    ON domain_events(transaction_id, sequence_number);

CREATE INDEX IF NOT EXISTS idx_domain_events_category
    ON domain_events(category, transaction_id, sequence_number);

CREATE INDEX IF NOT EXISTS idx_domain_events_created_at
    ON domain_events(created_at);
