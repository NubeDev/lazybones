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
