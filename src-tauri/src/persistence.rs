use rusqlite::Connection;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;
#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}
pub fn open_and_migrate(path: &Path) -> Result<Connection, PersistenceError> {
    let connection = Connection::open(path)?;
    // One local connection is shared by the account workers, the Market and
    // UI commands. WAL keeps short writes from blocking readers at the OS
    // level, while the bounded busy wait avoids an immediate failure during a
    // harmless close/reopen race of the desktop process.
    connection.busy_timeout(Duration::from_secs(3))?;
    connection.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;",
    )?;
    migrate(&connection)?;
    Ok(connection)
}
pub fn migrate(connection: &Connection) -> Result<(), PersistenceError> {
    connection.execute_batch(include_str!("../migrations/0001_initial.sql"))?;
    connection.execute(
        "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (1, unixepoch())",
        [],
    )?;
    connection.execute_batch(include_str!("../migrations/0002_market.sql"))?;
    connection.execute(
        "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (2, unixepoch())",
        [],
    )?;
    connection.execute_batch(include_str!(
        "../migrations/0003_market_purchase_history.sql"
    ))?;
    connection.execute(
        "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (3, unixepoch())",
        [],
    )?;
    connection.execute_batch(include_str!("../migrations/0004_market_global_history.sql"))?;
    connection.execute(
        "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (4, unixepoch())",
        [],
    )?;
    connection.execute_batch(include_str!("../migrations/0005_market_item_metadata.sql"))?;
    connection.execute(
        "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (5, unixepoch())",
        [],
    )?;
    let has_outcome = {
        let mut statement = connection.prepare("PRAGMA table_info(market_purchase_history)")?;
        let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
        columns
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|column| column == "outcome")
    };
    if !has_outcome {
        connection.execute_batch(include_str!(
            "../migrations/0006_market_purchase_outcomes.sql"
        ))?;
    }
    connection.execute(
        "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (6, unixepoch())",
        [],
    )?;
    connection.execute_batch(include_str!("../migrations/0007_account_hunt_state.sql"))?;
    connection.execute(
        "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (7, unixepoch())",
        [],
    )?;
    connection.execute_batch(include_str!("../migrations/0008_account_hunt_session.sql"))?;
    connection.execute(
        "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (8, unixepoch())",
        [],
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_migration_creates_account_tables() {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='accounts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn market_outcome_migration_is_idempotent_and_defaults_old_rows_to_purchased() {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        migrate(&connection).unwrap();
        let outcome_columns: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('market_purchase_history') WHERE name = 'outcome'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(outcome_columns, 1);
    }

    #[test]
    fn market_migration_creates_bounded_observation_storage() {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='market_observations'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn market_migration_creates_confirmed_purchase_history() {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='market_purchase_history'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn market_migration_creates_global_history_storage() {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='market_global_history'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn market_migration_creates_item_metadata_storage() {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='market_item_metadata'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn hunt_state_migration_is_idempotent_and_preserves_existing_accounts() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(include_str!("../migrations/0001_initial.sql"))
            .unwrap();
        for (version, migration) in [
            (2, include_str!("../migrations/0002_market.sql")),
            (
                3,
                include_str!("../migrations/0003_market_purchase_history.sql"),
            ),
            (
                4,
                include_str!("../migrations/0004_market_global_history.sql"),
            ),
            (
                5,
                include_str!("../migrations/0005_market_item_metadata.sql"),
            ),
            (
                6,
                include_str!("../migrations/0006_market_purchase_outcomes.sql"),
            ),
        ] {
            connection.execute_batch(migration).unwrap();
            connection
                .execute(
                    "INSERT INTO schema_migrations(version, applied_at) VALUES (?1, unixepoch())",
                    [version],
                )
                .unwrap();
        }
        connection
            .execute(
                "INSERT INTO accounts(id, nick, card_color, created_at) VALUES ('a', 'Alpha', '#fff', unixepoch())",
                [],
            )
            .unwrap();

        migrate(&connection).unwrap();
        migrate(&connection).unwrap();

        let account_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM accounts WHERE id = 'a'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let timer_table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'account_hunt_state'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let session_table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'account_hunt_session'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let migration_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = 7",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let session_migration_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = 8",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(account_count, 1);
        assert_eq!(timer_table_count, 1);
        assert_eq!(session_table_count, 1);
        assert_eq!(migration_count, 1);
        assert_eq!(session_migration_count, 1);
    }
}
