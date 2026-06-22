//! Parse a gated-action `CONFIRM:` envelope out of the agent's reply.
//!
//! Tool-agnostic by design (`docs/agent/lazybones-agent-scope.md` §11 Q3): rather
//! than a claude-specific tool call, an `AuthorAndManage` agent proposes a
//! lifecycle action by emitting a line `CONFIRM: {json}` on its hcom thread — the
//! same sentinel idiom task agents use for `DONE`/`BLOCKED`. The runner parses it,
//! persists a `confirm` message, and NEVER lets the agent take the action itself;
//! the UI issues the call under the operator's token (§10.2).

use lazybones_store::ConfirmAction;

/// What the runner extracted from one agent reply.
pub struct ParsedReply {
    /// The human-facing prose (the reply with any CONFIRM line removed). Empty if
    /// the reply was nothing but a CONFIRM envelope.
    pub text: String,
    /// A parsed gated action, if the reply carried a well-formed CONFIRM line.
    pub confirm: Option<(String, ConfirmAction)>,
}

/// The sentinel prefix an agent uses to propose a gated action.
const CONFIRM_PREFIX: &str = "CONFIRM:";

/// Split an agent reply into prose + an optional confirm action.
///
/// A line starting (after trimming) with `CONFIRM:` is treated as a gated-action
/// envelope; its remainder is parsed as JSON into a [`ConfirmAction`]. A
/// malformed envelope is left as prose (surfaced to the operator) rather than
/// silently dropped — better a visible "I tried to propose X" than a lost action.
#[must_use]
pub fn parse_reply(reply: &str) -> ParsedReply {
    let mut prose_lines = Vec::new();
    let mut confirm = None;

    for line in reply.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(CONFIRM_PREFIX) {
            match serde_json::from_str::<ConfirmAction>(rest.trim()) {
                Ok(action) if confirm.is_none() => {
                    let summary = summarize(&action);
                    confirm = Some((summary, action));
                    continue;
                }
                // A second CONFIRM or a malformed one: keep it as prose so the
                // operator can see what the agent attempted.
                _ => prose_lines.push(line),
            }
        } else {
            prose_lines.push(line);
        }
    }

    ParsedReply {
        text: prose_lines.join("\n").trim().to_owned(),
        confirm,
    }
}

/// A short human-readable summary for the confirm card, derived from the action.
fn summarize(action: &ConfirmAction) -> String {
    format!(
        "The agent wants to {} — {} {}",
        action.action, action.method, action.path
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_reply_has_no_confirm() {
        let p = parse_reply("Created `add-healthcheck` with 2 tasks. Press Start.");
        assert!(p.confirm.is_none());
        assert!(p.text.contains("Created"));
    }

    #[test]
    fn parses_confirm_envelope_and_strips_it() {
        let reply = "I'll start it for you.\nCONFIRM: {\"action\":\"start\",\"method\":\"POST\",\"path\":\"/workflows/x/start\"}";
        let p = parse_reply(reply);
        let (summary, action) = p.confirm.expect("should parse");
        assert_eq!(action.action, "start");
        assert_eq!(action.path, "/workflows/x/start");
        assert!(summary.contains("start"));
        assert_eq!(p.text, "I'll start it for you.");
    }

    #[test]
    fn malformed_confirm_stays_as_prose() {
        let reply = "CONFIRM: not json at all";
        let p = parse_reply(reply);
        assert!(p.confirm.is_none());
        assert!(p.text.contains("CONFIRM:"));
    }
}
