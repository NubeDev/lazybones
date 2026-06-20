//! `hcom events --wait <secs> --sql "<expr>"` — block until a matching event or
//! timeout.
//!
//! hcom prints one JSON object per matching event and exits 0 on a match; on
//! timeout it prints `{"timed_out": true}` and exits 1 (src/commands/events.rs,
//! `events_wait`). We treat a timeout as "no events", not an error.

use std::time::Duration;

use serde::Deserialize;

use super::Hcom;

/// One event hcom matched while waiting — the full parsed shape. `data` carries
/// the signal the scheduler acts on; the rest is kept for diagnostics/logging.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct HcomEvent {
    /// Event id (monotonic in hcom's local db).
    #[serde(default)]
    pub id: serde_json::Value,
    /// RFC3339 timestamp hcom stamped the event with. Kept for the hcom log's
    /// `at`; `None`/empty when hcom omits it.
    #[serde(default)]
    pub ts: String,
    /// Event type (`message`, `status`, `life`, …).
    #[serde(default, rename = "type")]
    pub kind: String,
    /// The emitting agent's name.
    #[serde(default)]
    pub instance: String,
    /// The event payload (message text, status, …).
    #[serde(default)]
    pub data: serde_json::Value,
}

impl HcomEvent {
    /// hcom's event id as an integer, if it is one. hcom emits `id` as a JSON
    /// number; this is the monotonic cursor the tail compares with `id >`. A
    /// non-integer id (shouldn't happen on 0.7.21) yields `None` and the event is
    /// skipped by the tail rather than mis-cursored.
    #[must_use]
    pub fn id_int(&self) -> Option<i64> {
        self.id.as_i64()
    }
}

impl Hcom {
    /// Block until an event matches `sql` or `timeout` elapses.
    ///
    /// Returns the matched events (empty on timeout). The `sql` is a raw WHERE
    /// clause over hcom's `events_v` view.
    ///
    /// # Errors
    /// Returns an error if hcom cannot be launched or reports a SQL error
    /// (exit code 2).
    pub async fn wait(&self, sql: &str, timeout: Duration) -> anyhow::Result<Vec<HcomEvent>> {
        let mut cmd = self.command();
        cmd.arg("events")
            .arg("--wait")
            .arg(timeout.as_secs().to_string())
            .arg("--sql")
            .arg(sql);

        let out = cmd.output().await?;
        // Exit 2 is a SQL error; 1 is a clean timeout; 0 is a match.
        if out.status.code() == Some(2) {
            anyhow::bail!(
                "hcom events --sql failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(parse_events(&String::from_utf8_lossy(&out.stdout)))
    }

    /// Non-blocking tail: every event with `id > cursor`, returned immediately.
    ///
    /// This is `wait()`'s `--sql "id > {cursor}"` clause with a `0`-second
    /// timeout, so hcom returns whatever's queued and exits rather than blocking
    /// (docs/hcom-logs-scope.md — the drain-per-tick the hcom log is built on).
    /// `cursor` is a `u64` we control (the stored `hcom_log_cursor`), never agent
    /// input, so interpolating it into the WHERE clause is safe — the same way
    /// `finish::await_signal` interpolates its own trusted values.
    ///
    /// # Errors
    /// Returns an error if hcom cannot be launched or reports a SQL error.
    pub async fn events_since(&self, cursor: u64) -> anyhow::Result<Vec<HcomEvent>> {
        self.wait(&format!("id > {cursor}"), Duration::from_secs(0))
            .await
    }
}

/// Parse hcom's line-delimited event JSON, dropping the timeout sentinel.
fn parse_events(stdout: &str) -> Vec<HcomEvent> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        // The timeout sentinel `{"timed_out": true}` deserialises into an empty
        // event; filter it out by checking for it explicitly first.
        .filter(|l| !l.contains("\"timed_out\""))
        .filter_map(|l| serde_json::from_str::<HcomEvent>(l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_events;

    #[test]
    fn parses_event_line() {
        let out = r#"{"id":7,"ts":"2026","type":"message","instance":"a","data":{"text":"DONE"}}"#;
        let events = parse_events(out);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "message");
        assert_eq!(events[0].instance, "a");
        assert_eq!(events[0].data["text"], "DONE");
    }

    #[test]
    fn drops_timeout_sentinel() {
        assert!(parse_events("{\"timed_out\": true}\n").is_empty());
    }
}
