//! The `http-fetch` host allowlist (design §3.3).
//!
//! `http-fetch` is outbound HTTP **restricted to an allowlist of hosts**. The
//! allowlist is the only thing standing between an extension and exfiltration —
//! design §3.3 calls out `secrets-read` + `http-fetch` as the classic
//! exfiltration pair — so it is enforced host-side, never delegated to the guest.
//!
//! This is the policy half: the set of permitted hosts and the
//! [`is_allowed`](HostAllowlist::is_allowed) decision a future `wasi:http`
//! outbound handler consults before dialling. Wiring it into the `wasi:http`
//! linker handler lands with that handler in a later task; the allowlist itself
//! is default-deny today (an empty allowlist permits nothing).

use std::collections::BTreeSet;

/// A default-deny set of hosts an extension's `http-fetch` may reach.
///
/// Matching is on the URL **host** (no scheme, no port, no path), compared
/// case-insensitively. An empty allowlist denies everything — granting
/// `http-fetch` with no hosts is a no-op, which is the safe default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostAllowlist {
    hosts: BTreeSet<String>,
}

impl HostAllowlist {
    /// An empty (deny-everything) allowlist.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build an allowlist from an iterator of host strings (each normalised to
    /// lowercase and trimmed; empties are dropped).
    pub fn from_hosts<I, S>(hosts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let hosts = hosts
            .into_iter()
            .map(|h| h.as_ref().trim().to_ascii_lowercase())
            .filter(|h| !h.is_empty())
            .collect();
        Self { hosts }
    }

    /// Whether `host` (a bare hostname, case-insensitive) is permitted.
    #[must_use]
    pub fn is_allowed(&self, host: &str) -> bool {
        self.hosts.contains(&host.trim().to_ascii_lowercase())
    }

    /// Whether the allowlist permits nothing (the default-deny state).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }

    /// The permitted hosts, sorted.
    pub fn hosts(&self) -> impl Iterator<Item = &str> {
        self.hosts.iter().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_allowlist_denies_everything() {
        let allow = HostAllowlist::new();
        assert!(allow.is_empty());
        assert!(!allow.is_allowed("example.com"));
    }

    #[test]
    fn matches_case_insensitively_and_ignores_blanks() {
        let allow = HostAllowlist::from_hosts(["Example.com", "  api.test  ", ""]);
        assert!(allow.is_allowed("example.com"));
        assert!(allow.is_allowed("EXAMPLE.COM"));
        assert!(allow.is_allowed("api.test"));
        assert!(!allow.is_allowed("evil.com"));
        // The blank entry did not widen the set.
        assert_eq!(allow.hosts().count(), 2);
    }
}
