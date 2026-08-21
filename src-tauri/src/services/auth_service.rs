//! Authentication & first-run initialization.
//!
//! First launch: `settings.is_initialized = 0` -> the frontend shows an
//! "create administrator" screen -> `initialize_admin` runs once, creates
//! the admin user, and flips `is_initialized` to 1 atomically. Every
//! launch after that goes through `login`.

use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{NaiveDateTime, Utc};
use rusqlite::Connection;

use crate::database::schema::user_role;
use crate::database::transactions::with_transaction;
use crate::errors::{AppError, AppResult};
use crate::models::User;

/// After this many consecutive failed attempts, the account locks.
/// Deliberately per-account, not per-process/IP — this is a local
/// desktop app talking to its own SQLite file, so there's no network
/// layer to rate-limit at. This is the only line of defense against
/// someone (or a script driving the UI) guessing a weak password with
/// an unattended machine.
const MAX_FAILED_ATTEMPTS: i64 = 5;

/// How long an account stays locked once it hits the threshold.
const LOCKOUT_MINUTES: i64 = 15;

const SQLITE_DATETIME_FMT: &str = "%Y-%m-%d %H:%M:%S";

pub fn hash_password(password: &str) -> AppResult<String> {
    if password.len() < 6 {
        return Err(AppError::Validation(
            "password must be at least 6 characters".into(),
        ));
    }
    hash(password, DEFAULT_COST).map_err(|e| AppError::Internal(e.to_string()))
}

fn verify_password(password: &str, password_hash: &str) -> AppResult<bool> {
    verify(password, password_hash).map_err(|e| AppError::Internal(e.to_string()))
}

/// Whether the system has completed first-run setup yet.
pub fn is_initialized(conn: &Connection) -> AppResult<bool> {
    let initialized: bool = conn.query_row(
        "SELECT is_initialized FROM settings WHERE id = 1",
        [],
        |row| row.get(0),
    )?;
    Ok(initialized)
}

/// First-launch setup: create the sole administrator account and mark
/// the system initialized. Runs in a transaction — either both succeed
/// or neither does, so the app can never end up "initialized" with no
/// admin able to log in.
pub fn initialize_admin(
    conn: &mut Connection,
    username: &str,
    password: &str,
    full_name: &str,
) -> AppResult<User> {
    if is_initialized(conn)? {
        return Err(AppError::AlreadyInitialized);
    }
    if username.trim().is_empty() || full_name.trim().is_empty() {
        return Err(AppError::Validation("username and full name are required".into()));
    }

    let password_hash = hash_password(password)?;

    with_transaction(conn, |tx| {
        tx.execute(
            "INSERT INTO users (username, password_hash, full_name, role) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![username, password_hash, full_name, user_role::ADMIN],
        )?;
        let user_id = tx.last_insert_rowid();

        tx.execute(
            "UPDATE settings SET is_initialized = 1, updated_at = datetime('now') WHERE id = 1",
            [],
        )?;

        let user = tx.query_row(
            "SELECT * FROM users WHERE id = ?1",
            [user_id],
            User::from_row,
        )?;
        Ok(user)
    })
}

/// Credential-based login. Returns the authenticated user, or a
/// deliberately generic `Auth` error (never reveals whether the
/// username or the password was wrong) — except for the lockout
/// message itself, which is intentionally specific: once someone's
/// past the point of guessing correctly, telling them how long to
/// wait isn't giving anything away, and it's much better UX than a
/// generic "invalid username or password" that leaves a locked-out
/// legitimate user thinking they've forgotten their own password.
pub fn login(conn: &Connection, username: &str, password: &str) -> AppResult<User> {
    let (user, mut failed_attempts, locked_until): (User, i64, Option<String>) = conn
        .query_row("SELECT * FROM users WHERE username = ?1", [username], |row| {
            Ok((
                User::from_row(row)?,
                row.get("failed_login_attempts")?,
                row.get("locked_until")?,
            ))
        })
        .map_err(|_| AppError::Auth("invalid username or password".into()))?;

    if let Some(locked_until) = locked_until {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(&locked_until, SQLITE_DATETIME_FMT) {
            let remaining = parsed - Utc::now().naive_utc();
            if remaining.num_seconds() > 0 {
                let minutes = ((remaining.num_seconds() as f64) / 60.0).ceil().max(1.0) as i64;
                return Err(AppError::Auth(format!(
                    "too many failed attempts — try again in {minutes} minute{}",
                    if minutes == 1 { "" } else { "s" }
                )));
            }
            // The lockout window has passed — give a clean slate rather
            // than letting one more failure instantly re-lock.
            clear_failed_attempts(conn, user.id)?;
            failed_attempts = 0;
        }
    }

    if !user.is_active {
        return Err(AppError::Auth("this account has been deactivated".into()));
    }

    if !verify_password(password, &user.password_hash)? {
        record_failed_attempt(conn, user.id, failed_attempts)?;
        return Err(AppError::Auth("invalid username or password".into()));
    }

    if failed_attempts > 0 {
        clear_failed_attempts(conn, user.id)?;
    }

    Ok(user)
}

fn record_failed_attempt(conn: &Connection, user_id: i64, previous_attempts: i64) -> AppResult<()> {
    let attempts = previous_attempts + 1;
    if attempts >= MAX_FAILED_ATTEMPTS {
        let locked_until = (Utc::now() + chrono::Duration::minutes(LOCKOUT_MINUTES))
            .format(SQLITE_DATETIME_FMT)
            .to_string();
        conn.execute(
            "UPDATE users SET failed_login_attempts = ?1, locked_until = ?2 WHERE id = ?3",
            rusqlite::params![attempts, locked_until, user_id],
        )?;
    } else {
        conn.execute(
            "UPDATE users SET failed_login_attempts = ?1 WHERE id = ?2",
            rusqlite::params![attempts, user_id],
        )?;
    }
    Ok(())
}

fn clear_failed_attempts(conn: &Connection, user_id: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE users SET failed_login_attempts = 0, locked_until = NULL WHERE id = ?1",
        [user_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn first_run_flow_then_login_succeeds() {
        let mut conn = db();
        assert!(!is_initialized(&conn).unwrap());

        let admin = initialize_admin(&mut conn, "admin", "supersecret", "Owner").unwrap();
        assert_eq!(admin.username, "admin");
        assert!(is_initialized(&conn).unwrap());

        let logged_in = login(&conn, "admin", "supersecret").unwrap();
        assert_eq!(logged_in.id, admin.id);
    }

    #[test]
    fn cannot_initialize_twice() {
        let mut conn = db();
        initialize_admin(&mut conn, "admin", "supersecret", "Owner").unwrap();
        let second = initialize_admin(&mut conn, "admin2", "supersecret", "Someone Else");
        assert!(matches!(second, Err(AppError::AlreadyInitialized)));
    }

    #[test]
    fn wrong_password_is_rejected() {
        let mut conn = db();
        initialize_admin(&mut conn, "admin", "supersecret", "Owner").unwrap();
        let result = login(&conn, "admin", "wrongpassword");
        assert!(matches!(result, Err(AppError::Auth(_))));
    }

    #[test]
    fn account_locks_after_max_failed_attempts() {
        let mut conn = db();
        initialize_admin(&mut conn, "admin", "supersecret", "Owner").unwrap();

        for _ in 0..MAX_FAILED_ATTEMPTS {
            let _ = login(&conn, "admin", "wrongpassword");
        }

        // Even the *correct* password must now be rejected — that's the
        // whole point of a lockout.
        let result = login(&conn, "admin", "supersecret");
        match result {
            Err(AppError::Auth(msg)) => assert!(msg.contains("try again"), "unexpected message: {msg}"),
            other => panic!("expected a lockout error, got {other:?}"),
        }
    }

    #[test]
    fn successful_login_resets_the_failed_attempt_counter() {
        let mut conn = db();
        initialize_admin(&mut conn, "admin", "supersecret", "Owner").unwrap();

        // A few failures, but not enough to lock.
        for _ in 0..(MAX_FAILED_ATTEMPTS - 1) {
            let _ = login(&conn, "admin", "wrongpassword");
        }
        login(&conn, "admin", "supersecret").unwrap();

        let attempts: i64 = conn
            .query_row("SELECT failed_login_attempts FROM users WHERE username = 'admin'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(attempts, 0);
    }

    #[test]
    fn lockout_expiry_gives_a_clean_slate() {
        let mut conn = db();
        initialize_admin(&mut conn, "admin", "supersecret", "Owner").unwrap();

        for _ in 0..MAX_FAILED_ATTEMPTS {
            let _ = login(&conn, "admin", "wrongpassword");
        }
        // Simulate the lockout window having already passed.
        conn.execute(
            "UPDATE users SET locked_until = datetime('now', '-1 minute') WHERE username = 'admin'",
            [],
        )
        .unwrap();

        // The correct password now works again, without needing to wait
        // for another MAX_FAILED_ATTEMPTS worth of tries.
        let user = login(&conn, "admin", "supersecret").unwrap();
        assert_eq!(user.username, "admin");

        let (attempts, locked_until): (i64, Option<String>) = conn
            .query_row(
                "SELECT failed_login_attempts, locked_until FROM users WHERE username = 'admin'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(attempts, 0);
        assert!(locked_until.is_none());
    }

    #[test]
    fn unknown_username_does_not_panic_or_touch_lockout_state() {
        let conn = db();
        let result = login(&conn, "nobody", "whatever");
        assert!(matches!(result, Err(AppError::Auth(_))));
    }

    #[test]
    fn failed_init_does_not_partially_apply() {
        // password too short -> hash_password fails before the transaction
        // even starts, so neither the user nor the settings flip happens.
        let mut conn = db();
        let result = initialize_admin(&mut conn, "admin", "abc", "Owner");
        assert!(result.is_err());
        assert!(!is_initialized(&conn).unwrap());
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0);
    }
}
