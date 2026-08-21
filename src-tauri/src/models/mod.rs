//! Domain models — one struct per table in /migrations.
//!
//! Every model exposes a `from_row(&rusqlite::Row) -> rusqlite::Result<Self>`
//! associated function so it can be used directly with `query_map`, e.g.:
//!
//! ```ignore
//! stmt.query_map([], Waiter::from_row)?
//! ```
//!
//! Columns whose values are constrained by a SQL CHECK (item.type,
//! sales.payment_method, users.role) are modeled as small enums via the
//! `sql_enum!` macro below, so an invalid value can't silently make it
//! into application code — it's rejected the moment it's read or written.

macro_rules! sql_enum {
    ($name:ident { $($variant:ident => $str:expr),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $str),+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = crate::errors::AppError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($str => Ok(Self::$variant),)+
                    other => Err(crate::errors::AppError::Validation(
                        format!("{}: invalid value '{}'", stringify!($name), other)
                    )),
                }
            }
        }

        impl rusqlite::types::FromSql for $name {
            fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
                let s = value.as_str()?;
                s.parse::<$name>().map_err(|e| rusqlite::types::FromSqlError::Other(Box::new(e)))
            }
        }

        impl rusqlite::types::ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(rusqlite::types::ToSqlOutput::from(self.as_str()))
            }
        }
    };
}

pub(crate) use sql_enum;

pub mod category;
pub mod expense;
pub mod item;
pub mod raw_material;
pub mod sale;
pub mod sale_item;
pub mod settings;
pub mod user;
pub mod waiter;

pub use category::Category;
pub use expense::Expense;
pub use item::{Item, ItemType};
pub use raw_material::{RawMaterial, RawMaterialPurchase};
pub use sale::{PaymentMethod, Sale};
pub use sale_item::SaleItem;
pub use settings::Settings;
pub use user::{User, UserRole};
pub use waiter::Waiter;
