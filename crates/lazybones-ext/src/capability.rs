//! The host capability vocabulary (design §3.3).
//!
//! Default-deny: a guest only ever runs against the capabilities its manifest
//! *requests* **and** an admin *grants* at install time. This module owns the
//! typed enum and the two policies that bound it:
//!
//! - **availability** — which capabilities v1 actually ships. `log`,
//!   `store-read`, and `http-fetch` are live; `store-write`, `secrets-read`, and
//!   `kv` are **deferred** (design §3.3/§3.7) and may not be requested or granted
//!   yet. `emit-event` is part of the design's eventual set but is likewise not
//!   wired in v1.
//! - **`granted ⊆ requested`** — [`validate_grant`] enforces that an admin can
//!   only grant capabilities the manifest declared, and never a deferred one.
//!
//! Capabilities cross the store boundary as wire strings (the store stays
//! decoupled from this enum); this is the one place the string⇄enum mapping and
//! the policy live.

use std::fmt;

/// A host capability a guest may be granted (design §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Structured tracing into the daemon. **Always** available and implicitly
    /// granted to every extension (design §3.3: "log (always)").
    Log,
    /// Read-only, typed, projected access to tasks/runs — the `ext-task-view` /
    /// `ext-run-view` facade, **never** raw SurrealDB rows (design §3.7).
    StoreRead,
    /// Outbound HTTP restricted to a host allowlist (design §3.3).
    HttpFetch,
    /// Typed write-back to the store. **Deferred** in v1 — write happens through
    /// typed extension-point return values, not arbitrary mutation (design §3.7).
    StoreWrite,
    /// Read named, host-decrypted secrets. **Deferred** in v1 (design §3.3: the
    /// `secrets-read` + `http-fetch` exfiltration pair needs the louder grant UI
    /// and signing story first).
    SecretsRead,
    /// Per-extension namespaced key/value scratch space. **Deferred** in v1.
    Kv,
    /// Append an extension-namespaced event. **Deferred** in v1 (needs the
    /// reentrancy/cycle guard from design §3.4 first).
    EmitEvent,
}

/// An unrecognised capability string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown capability: {0}")]
pub struct CapabilityError(pub String);

/// A grant was rejected by [`validate_grant`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GrantError {
    /// A granted capability was not declared in the manifest's requested set
    /// (`granted ⊆ requested` violated — design §3.5).
    #[error("granted capability `{0}` was not requested by the manifest")]
    NotRequested(Capability),

    /// A granted (or requested) capability is deferred and unavailable in v1.
    #[error("capability `{0}` is deferred and cannot be granted in v1")]
    Deferred(Capability),
}

impl Capability {
    /// The wire/storage string for this capability.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Log => "log",
            Capability::StoreRead => "store-read",
            Capability::HttpFetch => "http-fetch",
            Capability::StoreWrite => "store-write",
            Capability::SecretsRead => "secrets-read",
            Capability::Kv => "kv",
            Capability::EmitEvent => "emit-event",
        }
    }

    /// Parse a wire string into a capability.
    ///
    /// # Errors
    /// [`CapabilityError`] if the string names no known capability.
    pub fn parse(s: &str) -> Result<Self, CapabilityError> {
        match s {
            "log" => Ok(Capability::Log),
            "store-read" => Ok(Capability::StoreRead),
            "http-fetch" => Ok(Capability::HttpFetch),
            "store-write" => Ok(Capability::StoreWrite),
            "secrets-read" => Ok(Capability::SecretsRead),
            "kv" => Ok(Capability::Kv),
            "emit-event" => Ok(Capability::EmitEvent),
            other => Err(CapabilityError(other.to_owned())),
        }
    }

    /// Whether this capability is wired and grantable in v1. Deferred
    /// capabilities (`store-write`, `secrets-read`, `kv`, `emit-event`) return
    /// `false` (design §3.3/§3.7).
    #[must_use]
    pub fn is_available(self) -> bool {
        matches!(
            self,
            Capability::Log | Capability::StoreRead | Capability::HttpFetch
        )
    }

    /// Whether this capability is granted to *every* extension regardless of the
    /// admin grant set. Only `log` is (design §3.3: "log (always)").
    #[must_use]
    pub fn is_always_granted(self) -> bool {
        matches!(self, Capability::Log)
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Enforce the install-time grant policy: every `granted` capability must be
/// available in v1 **and** present in `requested` (`granted ⊆ requested`, design
/// §3.5). `log` is implicitly always granted, so it need not appear in either
/// set.
///
/// # Errors
/// [`GrantError::Deferred`] if a granted capability is not v1-available, or
/// [`GrantError::NotRequested`] if it was not in the manifest's requested set.
pub fn validate_grant(
    requested: &[Capability],
    granted: &[Capability],
) -> Result<(), GrantError> {
    for &cap in granted {
        if cap.is_always_granted() {
            continue;
        }
        if !cap.is_available() {
            return Err(GrantError::Deferred(cap));
        }
        if !requested.contains(&cap) {
            return Err(GrantError::NotRequested(cap));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_known_caps() {
        for cap in [
            Capability::Log,
            Capability::StoreRead,
            Capability::HttpFetch,
            Capability::StoreWrite,
            Capability::SecretsRead,
            Capability::Kv,
            Capability::EmitEvent,
        ] {
            assert_eq!(Capability::parse(cap.as_str()).unwrap(), cap);
        }
        assert!(Capability::parse("bogus").is_err());
    }

    #[test]
    fn v1_availability_split() {
        assert!(Capability::Log.is_available());
        assert!(Capability::StoreRead.is_available());
        assert!(Capability::HttpFetch.is_available());
        assert!(!Capability::StoreWrite.is_available());
        assert!(!Capability::SecretsRead.is_available());
        assert!(!Capability::Kv.is_available());
        assert!(!Capability::EmitEvent.is_available());
    }

    #[test]
    fn grant_must_be_subset_of_requested() {
        let requested = vec![Capability::StoreRead, Capability::HttpFetch];
        // Subset is fine.
        validate_grant(&requested, &[Capability::StoreRead]).unwrap();
        // log is always allowed even if not requested.
        validate_grant(&requested, &[Capability::Log]).unwrap();
        // Granting something not requested is rejected.
        assert_eq!(
            validate_grant(&requested, &[Capability::HttpFetch, Capability::StoreRead]),
            Ok(())
        );
        assert_eq!(
            validate_grant(&[Capability::StoreRead], &[Capability::HttpFetch]),
            Err(GrantError::NotRequested(Capability::HttpFetch))
        );
        // A deferred cap can never be granted.
        assert_eq!(
            validate_grant(&[Capability::StoreWrite], &[Capability::StoreWrite]),
            Err(GrantError::Deferred(Capability::StoreWrite))
        );
    }
}
