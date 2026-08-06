//! Database Facade Layer
//!
//! Handles database connections, transactions, and basic initialization.
//! Business logic is migrated to Services; SQL to Repositories.
//! Strictly follows Clean Architecture: Commands -> Services -> Repositories -> DB

pub(crate) mod migrations;
use rusqlite::Connection;
use std::path::PathBuf;

use crate::errors::AppError;

// Type alias for database results using AppError
pub type DbResult<T> = Result<T, AppError>;

/// Database connection and transaction coordinator.
/// In Clean Architecture, this struct should only manage life-cycle and transactions.
pub struct Database {
    pub(crate) conn: Connection,
}

impl Database {
    /// Get the raw rusqlite connection
    pub fn get_connection(&self) -> &Connection {
        &self.conn
    }

    /// Get a DbExecutor wrapping the connection (no transaction)
    pub fn executor(&self) -> crate::repositories::DbExecutor<'_> {
        crate::repositories::DbExecutor::Conn(&self.conn)
    }

    /// Execute within a new transaction, committing on success
    pub fn with_transaction<F, T>(&mut self, f: F) -> DbResult<T>
    where
        F: FnOnce(crate::repositories::DbExecutor<'_>) -> DbResult<T>,
    {
        crate::infrastructure::db::transaction::TransactionService::with_transaction(
            &mut self.conn,
            f,
        )
    }

    /// Execute within a transaction with domain event support.
    ///
    /// Provides an `EventContext` for database access and event emission.
    /// On success returns `(result, EventBuffer)`; on rollback the buffer is discarded.
    ///
    /// Events are NOT automatically persisted. Use `with_event_persistence`
    /// for automatic persistence before commit.
    pub fn with_event_context<F, T>(
        &mut self,
        f: F,
    ) -> Result<(T, crate::domain::events::EventBuffer), AppError>
    where
        F: FnOnce(&mut crate::domain::events::EventContext<'_>) -> Result<T, AppError>,
    {
        crate::infrastructure::db::transaction::TransactionService::with_event_context(
            &mut self.conn,
            f,
        )
    }

    /// Execute within a transaction with automatic event persistence.
    ///
    /// Works like `with_event_context`, but additionally persists all
    /// buffered events to the `domain_events` table before committing.
    ///
    /// Persistence guarantees:
    /// - Events are persisted inside the same SQLite transaction
    /// - If the operation rolls back, events are discarded
    /// - If commit succeeds, events are atomically persisted with state changes
    pub fn with_event_persistence<F, T>(
        &mut self,
        f: F,
    ) -> Result<(T, crate::domain::events::EventBuffer), AppError>
    where
        F: FnOnce(&mut crate::domain::events::EventContext<'_>) -> Result<T, AppError>,
    {
        crate::infrastructure::db::transaction::TransactionService::with_event_persistence(
            &mut self.conn,
            f,
        )
    }

    /// الحصول على مسار ملف قاعدة البيانات
    pub fn get_connection_path(&self) -> DbResult<PathBuf> {
        Ok(get_connection_path(&self.conn)?)
    }

    /// Close the database connection gracefully
    pub fn close(self) {
        // Connection will be closed when Database is dropped
        drop(self.conn);
    }
}

// Infrastructure Layer (EXCEPTION)
// ALLOWED: PRAGMA & MIGRATIONS are infrastructure-level SQL

use dirs::data_dir;
use std::fs;

/// Apply PRAGMA settings for performance and safety
pub fn apply_pragma_settings(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        -- Transaction safety
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = FULL;
        
        -- Referential integrity enforcement
        PRAGMA foreign_keys = ON;
        
        -- SECURITY: Disable potentially dangerous features
        PRAGMA trusted_schema = OFF;
        
        -- Memory settings
        PRAGMA temp_store = MEMORY;
        PRAGMA cache_size = -8192;
        
        -- Query optimization
        PRAGMA query_only = OFF;
        PRAGMA automatic_index = ON;
    "#,
    )
    .map_err(|e| format!("Failed to set PRAGMA: {}", e))?;

    Ok(())
}

/// Apply simplified PRAGMA settings for test databases.
pub fn apply_test_pragma_settings(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        -- Basic settings for testing
        PRAGMA synchronous = OFF;
        PRAGMA journal_mode = DELETE;
        PRAGMA foreign_keys = ON;
        PRAGMA temp_store = MEMORY;
        PRAGMA cache_size = 1000;
    "#,
    )
    .map_err(|e| format!("Failed to set test PRAGMA: {}", e))?;
    Ok(())
}

/// Run database migrations
pub fn run_migrations(conn: &Connection) -> Result<(), String> {
    crate::db::migrations::run_migrations(conn)
}

/// Get the database path
pub fn get_db_path() -> crate::errors::AppResult<PathBuf> {
    // Allow overriding the database path via environment variable (for testing)
    if let Ok(custom_path) = std::env::var("GRPC_DB_PATH") {
        let path = PathBuf::from(custom_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AppError::Internal(format!("Failed to create DB directory: {}", e)))?;
        }
        return Ok(path);
    }

    let app_data = data_dir()
        .ok_or_else(|| AppError::Internal("Could not find data directory".into()))?
        .join("GRPC");

    fs::create_dir_all(&app_data)
        .map_err(|e| AppError::Internal(format!("Failed to create app data directory: {}", e)))?;
    Ok(app_data.join("grpc.db"))
}

/// Read active database file path from SQLite metadata.
pub fn get_connection_path(conn: &Connection) -> Result<PathBuf, rusqlite::Error> {
    let path: String = conn.query_row("PRAGMA database_list", [], |row| row.get(2))?;
    Ok(PathBuf::from(path))
}

use rusqlite::Connection as SqlConnection;
use std::path::Path;

/// TEST-SUPPORT ONLY (B6-A): seed the legacy `admin/admin` account into a
/// database.
///
/// RFC 2026-08-04 §3.6 / ADR-0038. Production startup NO LONGER seeds a
/// default admin (removed in B6-A) — fresh fleets bootstrap through the
/// offline Root flow. This helper exists exclusively for the in-memory / temp
/// test factories (`new_for_test`, `new_with_path`) so integration tests keep
/// a known credential while the DB is `Uninitialized` (the B6-A login gate
/// leaves the password path open on such databases).
///
/// B8: the seeded admin is the fleet-wide synchronized `admin`, so its hash is
/// derived in the global admin domain (identical on every node).
///
/// Idempotent by construction: an existing `admin` row is never overwritten.
pub fn seed_default_admin(db: &Database) -> crate::errors::AppResult<()> {
    use crate::domain::security::PasswordHashPort;
    use crate::infrastructure::security::node_identity_provider::NodeIdentityProvider;
    use crate::infrastructure::security::SettingsNodeIdentityProvider;
    use crate::models::UserRole;
    use crate::repositories::RepositoryProvider;

    let existing = db.executor().users().get_user_by_username("admin")?;
    if existing.is_some() {
        return Ok(());
    }

    let node_id = SettingsNodeIdentityProvider::new(db.executor())
        .current_node_id()
        .unwrap_or_else(|_| "WILAYA".to_string());
    let password_port = crate::infrastructure::security::Argon2PasswordHashProvider;
    let password_hash = password_port
        .hash_admin("admin")
        .map_err(crate::errors::AppError::Internal)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    db.executor().users().insert_user_if_absent(
        &id,
        "admin",
        &password_hash,
        UserRole::Admin,
        &node_id,
        &now,
    )?;
    Ok(())
}

/// Factory responsible for creating `Database` instances.
///
/// All PRAGMA + migration + bootstrap behavior is considered infrastructure-level.
pub struct ConnectionFactory;

impl ConnectionFactory {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> crate::errors::AppResult<Database> {
        crate::infrastructure::security::resolve_app_encryption_key()
            .map_err(|e| AppError::Internal(format!("Security key bootstrap failed: {}", e)))?;

        let db_path = get_db_path()?;
        let conn = SqlConnection::open(&db_path)?;

        // Apply PRAGMA settings
        apply_pragma_settings(&conn)
            .map_err(|e| AppError::Internal(format!("Failed to apply PRAGMA settings: {}", e)))?;

        // Run migrations
        run_migrations(&conn)
            .map_err(|e| AppError::Internal(format!("Migration failed: {}", e)))?;

        let db = Database { conn };
        Ok(db)
    }

    /// Create database for testing with simplified PRAGMA settings to avoid locking issues.
    #[allow(clippy::new_ret_no_self)]
    pub fn new_for_test() -> crate::errors::AppResult<Database> {
        // Generate a completely unique database path using timestamp and random number
        use std::time::{SystemTime, UNIX_EPOCH};

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let random_num: u32 = rand::random();

        // Create a unique temporary directory for this test
        let temp_dir = std::env::temp_dir();
        let test_db_dir = temp_dir.join(format!("grpc_test_{}_{}", timestamp, random_num));
        std::fs::create_dir_all(&test_db_dir)
            .map_err(|e| AppError::Internal(format!("Failed to create test DB dir: {}", e)))?;

        let db_path = test_db_dir.join("test.db");

        // Delegate to new_with_path to avoid set_var and logic duplication
        Self::new_with_path(&db_path)
    }

    /// Create database with a specific path for testing.
    ///
    /// # Security
    /// This method is intended for testing only.
    #[allow(clippy::new_ret_no_self)]
    pub fn new_with_path(db_path: &Path) -> crate::errors::AppResult<Database> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Internal(format!("Failed to create parent dir: {}", e)))?;
        }

        let conn = SqlConnection::open(db_path)?;

        // Apply simplified PRAGMA settings for testing
        apply_test_pragma_settings(&conn).map_err(|e| AppError::Internal(e.to_string()))?;

        // Run migrations
        run_migrations(&conn)
            .map_err(|e| AppError::Internal(format!("Migration failed: {}", e)))?;

        let db = Database { conn };
        seed_default_admin(&db).map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(db)
    }
}

use rusqlite::{Result as SqliteResult, Row, Rows};

/// Row mapping collector for repositories.
///
/// Architectural intent:
/// - repositories provide SQL + a row-mapping closure
/// - the actual row iteration loop lives here (db/infrastructure)
///
/// Memory Safety:
/// This method collects all results into a Vec. This is acceptable for the project's target
/// (medium datasets, ~100 nodes). For potentially massive tables, use `query_iter` in DbExecutor.
pub(crate) fn map_rows_all<T, F>(rows: &mut Rows<'_>, mut f: F) -> SqliteResult<Vec<T>>
where
    F: FnMut(&Row<'_>) -> SqliteResult<T>,
{
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(f(row)?);
    }
    Ok(out)
}
