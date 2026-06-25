//! Normalize a user-entered remote into a canonical, `gh`-auth-friendly URL, and
//! derive the per-provider checkout sub-dir.
//!
//! Content sync borrows `gh`'s auth, which is an **HTTPS token** (set up once with
//! `gh auth login`), not an SSH key. So we canonicalise every accepted input form
//! to an `https://…​.git` URL that `gh`'s git credential helper serves — that's
//! why an `git@github.com:…` SSH URL (which needs SSH keys the user may not have)
//! is rewritten to HTTPS rather than used as-is. Accepted inputs:
//!
//! - `NubeDev/lb-sync`                      (owner/repo shorthand → github)
//! - `https://github.com/NubeDev/lb-sync`   (with or without `.git`)
//! - `git@github.com:NubeDev/lb-sync.git`   (SSH → rewritten to HTTPS)
//! - `ssh://git@github.com/NubeDev/lb-sync` (SSH → rewritten to HTTPS)

/// Canonicalise `input` to an `https://host/owner/repo.git` URL. Returns an empty
/// string for empty input. A form we don't recognise is returned trimmed but
/// otherwise untouched (so a self-hosted/other URL still works).
#[must_use]
pub fn normalize_remote(input: &str) -> String {
    let s = input.trim().trim_end_matches('/');
    if s.is_empty() {
        return String::new();
    }

    // `git@host:owner/repo(.git)` — the classic SSH scp-like form.
    if let Some(rest) = s.strip_prefix("git@")
        && let Some((host, path)) = rest.split_once(':')
    {
        return format!("https://{host}/{}.git", strip_dot_git(path));
    }

    // `ssh://[git@]host/owner/repo(.git)`
    if let Some(rest) = s.strip_prefix("ssh://") {
        let rest = rest.strip_prefix("git@").unwrap_or(rest);
        return format!("https://{}.git", strip_dot_git(rest));
    }

    // Already an HTTP(S) URL — just ensure the `.git` suffix.
    if s.starts_with("https://") || s.starts_with("http://") {
        return format!("{}.git", strip_dot_git(s));
    }

    // `owner/repo` shorthand (exactly one slash, no scheme/host) → github.
    if !s.contains("://") && !s.contains('@') && s.matches('/').count() == 1 {
        return format!("https://github.com/{}.git", strip_dot_git(s));
    }

    s.to_owned()
}

/// A short slug naming the host a remote lives on, used to namespace the local
/// checkout dir (`<data_dir>/sync/<slug>`) so different providers — with
/// different auth — never share a working tree.
#[must_use]
pub fn provider_slug(remote: &str) -> &'static str {
    let r = remote.to_ascii_lowercase();
    if r.contains("github.com") {
        "gh"
    } else if r.contains("gitlab") {
        "gl"
    } else if r.contains("bitbucket") {
        "bb"
    } else {
        "git"
    }
}

/// Drop a trailing `.git` (so we can re-add it uniformly).
fn strip_dot_git(s: &str) -> &str {
    s.strip_suffix(".git").unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_every_accepted_form_to_https_dot_git() {
        let want = "https://github.com/NubeDev/lb-sync.git";
        for input in [
            "NubeDev/lb-sync",
            "https://github.com/NubeDev/lb-sync",
            "https://github.com/NubeDev/lb-sync.git",
            "https://github.com/NubeDev/lb-sync/",
            "git@github.com:NubeDev/lb-sync.git",
            "git@github.com:NubeDev/lb-sync",
            "ssh://git@github.com/NubeDev/lb-sync.git",
        ] {
            assert_eq!(normalize_remote(input), want, "input: {input}");
        }
    }

    #[test]
    fn empty_and_unknown_forms() {
        assert_eq!(normalize_remote("   "), "");
        // A non-github host SSH URL still rewrites to https on that host.
        assert_eq!(
            normalize_remote("git@gitlab.com:me/repo.git"),
            "https://gitlab.com/me/repo.git"
        );
        // Something with no recognisable shape is left alone (trimmed).
        assert_eq!(normalize_remote("  weird-thing  "), "weird-thing");
    }

    #[test]
    fn provider_slugs_namespace_the_checkout() {
        assert_eq!(provider_slug("https://github.com/a/b.git"), "gh");
        assert_eq!(provider_slug("https://gitlab.com/a/b.git"), "gl");
        assert_eq!(provider_slug("https://bitbucket.org/a/b.git"), "bb");
        assert_eq!(provider_slug("https://example.com/a/b.git"), "git");
    }
}
