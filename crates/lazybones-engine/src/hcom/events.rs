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
    /// NOTE: this does **not** use `--wait`. On hcom 0.7.21 `--wait N` blocks for
    /// `N` seconds waiting for a *new* matching event — it does **not** replay the
    /// existing backlog — so `--wait 0` always returns the `{"timed_out":true}`
    /// sentinel (exit 1) and drains nothing, which is what kept the hcom log empty.
    /// The backlog form is `events --sql "id > {cursor}" --last N` (no `--wait`),
    /// which returns the queued events and exits 0. We cap with a large `--last`
    /// (`DRAIN_CAP`) and warn if a single tick's backlog hits the cap, since that
    /// means events older than the newest `DRAIN_CAP` were skipped this pass.
    ///
    /// `cursor` is a `u64` we control (the stored `hcom_log_cursor`), never agent
    /// input, so interpolating it into the WHERE clause is safe — the same way
    /// `finish::await_signal` interpolates its own trusted values.
    ///
    /// # Errors
    /// Returns an error if hcom cannot be launched or reports a SQL error.
    pub async fn events_since(&self, cursor: u64) -> anyhow::Result<Vec<HcomEvent>> {
        let mut cmd = self.command();
        cmd.args(Self::events_since_argv(cursor));

        let out = cmd.output().await?;
        // Exit 2 is a SQL error; without --wait there is no timeout exit.
        if out.status.code() == Some(2) {
            anyhow::bail!(
                "hcom events --sql failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let events = parse_events(&String::from_utf8_lossy(&out.stdout));
        if events.len() >= DRAIN_CAP {
            tracing::warn!(
                cursor,
                cap = DRAIN_CAP,
                "tail: drain hit --last cap; events older than this batch may be skipped"
            );
        }
        Ok(events)
    }

    /// The id of the newest event hcom currently knows about, or `0` if there are
    /// none. Used by the management runner to pin a cursor *before* it spawns the
    /// agent, so its incremental drain (`events_since`) sees only this turn's
    /// output and never replays the conversation's backlog.
    ///
    /// # Errors
    /// Returns an error if hcom cannot be launched or reports a SQL error.
    pub async fn latest_event_id(&self) -> anyhow::Result<u64> {
        let mut cmd = self.command();
        cmd.arg("events")
            .arg("--sql")
            .arg("1=1")
            .arg("--last")
            .arg("1");
        let out = cmd.output().await?;
        if out.status.code() == Some(2) {
            anyhow::bail!(
                "hcom events --sql failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let latest = parse_events(&String::from_utf8_lossy(&out.stdout))
            .iter()
            .filter_map(HcomEvent::id_int)
            .map(|id| u64::try_from(id).unwrap_or(0))
            .max()
            .unwrap_or(0);
        Ok(latest)
    }

    /// The argv for the backlog drain — `events --sql "id > {cursor}" --last
    /// {DRAIN_CAP}`, no `--wait`. Factored out so the no-`--wait` contract is
    /// unit-testable without spawning hcom.
    fn events_since_argv(cursor: u64) -> [String; 5] {
        [
            "events".to_owned(),
            "--sql".to_owned(),
            format!("id > {cursor}"),
            "--last".to_owned(),
            DRAIN_CAP.to_string(),
        ]
    }
}

/// Max events pulled in a single tick's drain (`--last`). Ticks are frequent and
/// the per-tick backlog is small, so this is a generous ceiling; hitting it warns
/// (see `events_since`) rather than silently truncating.
const DRAIN_CAP: usize = 5000;

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

    // Regression guard for the empty-hcom-log bug: the drain must NOT carry
    // `--wait`. On hcom 0.7.21 `--wait N` blocks for new events and returns the
    // `{"timed_out":true}` sentinel on a 0s timeout instead of replaying the
    // backlog, so `events_since` drained nothing and the hcom log stayed empty.
    // The backlog form is `events --sql "id > N" --last M` (no `--wait`).
    #[test]
    fn events_since_does_not_wait_and_caps_with_last() {
        let argv = super::Hcom::events_since_argv(42);
        assert!(
            !argv.iter().any(|a| a == "--wait"),
            "drain must not use --wait (it returns the timeout sentinel, not the backlog): {argv:?}"
        );
        assert!(
            argv.iter().any(|a| a == "id > 42"),
            "must filter id > cursor: {argv:?}"
        );
        let last = argv
            .iter()
            .position(|a| a == "--last")
            .expect("must bound with --last");
        assert_eq!(argv[last + 1], super::DRAIN_CAP.to_string());
    }
}
