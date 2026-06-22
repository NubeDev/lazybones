//! The user-preferences model — a single global record of operator UI choices.
//!
//! One preferences record per install (single-user daemon). It holds the
//! display/UX choices the operator makes in the settings page that should
//! follow them across browsers and devices, rather than living in each
//! browser's `localStorage`. The timezone is the first such field; the struct
//! is laid out so new preferences can be added as optional columns without a
//! migration.

use serde::{Deserialize, Serialize};

/// The single global user-preferences record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    /// IANA timezone name all dates/times are shown in (e.g.
    /// `"Asia/Ho_Chi_Minh"`). `None` (or empty) means "follow the browser".
    pub timezone: Option<String>,
    /// UI theme: `"light" | "dark" | "system"`. `None` means "system".
    pub theme: Option<String>,
    /// RFC3339 timestamp of the last write.
    pub updated_at: String,
}

impl Default for Preferences {
    /// The defaults the API returns when the operator has never saved any: no
    /// timezone override (follow the browser) and the system theme.
    fn default() -> Self {
        Self {
            timezone: None,
            theme: None,
            updated_at: String::new(),
        }
    }
}
