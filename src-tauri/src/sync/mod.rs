//! Optional Turso cloud sync.
//!
//! The app is fully functional offline with this module doing nothing
//! at all — `settings.sync_enabled` (see `services::settings_service`)
//! gates every bit of it. When it's on:
//!
//! - `queue` is the local outbox (`sync_queue` table): every synced
//!   mutation lands here first, durably, before it ever touches the
//!   network.
//! - `turso` is the remote side: connecting, mirroring schema, and the
//!   actual push/pull SQL.
//! - `conflict` decides who wins when the same row changed on two
//!   devices (last-write-wins on `updated_at`).
//! - `manager` ties the above into one push+pull cycle and the
//!   periodic background loop that runs it automatically.

pub mod conflict;
pub mod manager;
pub mod queue;
pub mod turso;

pub use manager::{run_once, start_background, SyncReport};
