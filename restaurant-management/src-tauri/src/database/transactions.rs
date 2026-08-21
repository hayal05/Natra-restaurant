//! Transaction helper.
//!
//! Wraps a closure in a SQLite transaction: commits on `Ok`, rolls back
//! automatically (via `rusqlite::Transaction`'s Drop) on `Err`. Used by
//! services that need to write across multiple tables atomically — e.g.
//! a POS sale, which inserts one `sales` row and several `sale_items`
//! rows and must never leave a half-written transaction.

use crate::errors::AppResult;
use rusqlite::Connection;

pub fn with_transaction<F, T>(conn: &mut Connection, f: F) -> AppResult<T>
where
    F: FnOnce(&rusqlite::Transaction) -> AppResult<T>,
{
    let tx = conn.transaction()?;
    let result = f(&tx)?;
    tx.commit()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;

    #[test]
    fn rolls_back_on_error() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrations::run(&conn).unwrap();

        conn.execute("INSERT INTO waiters (full_name) VALUES ('Alex')", [])
            .unwrap();

        // Attempt a transaction that inserts a waiter, then fails.
        let outcome: AppResult<()> = with_transaction(&mut conn, |tx| {
            tx.execute("INSERT INTO waiters (full_name) VALUES ('Ghost')", [])?;
            Err(crate::errors::AppError::Validation("forced failure".into()))
        });
        assert!(outcome.is_err());

        // "Ghost" must NOT exist — the transaction should have rolled back.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM waiters", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "only 'Alex' should remain after rollback");
    }

    #[test]
    fn commits_on_success() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrations::run(&conn).unwrap();

        let outcome: AppResult<()> = with_transaction(&mut conn, |tx| {
            tx.execute("INSERT INTO waiters (full_name) VALUES ('Alex')", [])?;
            Ok(())
        });
        assert!(outcome.is_ok());

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM waiters", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
