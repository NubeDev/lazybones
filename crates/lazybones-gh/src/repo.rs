use serde::Deserialize;

/// `gh repo view --json ...` result.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoView {
    pub name: String,
    pub owner: Owner,
    pub url: String,
    #[serde(default)]
    pub description: String,
    /// Default branch, e.g. `master` / `main`.
    #[serde(default, rename = "defaultBranchRef")]
    pub default_branch_ref: Option<Ref>,
}

impl RepoView {
    /// Convenience: `owner/name`.
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner.login, self.name)
    }

    /// Default branch name, if known.
    pub fn default_branch(&self) -> Option<&str> {
        self.default_branch_ref.as_ref().map(|r| r.name.as_str())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Owner {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Ref {
    pub name: String,
}

/// One branch, as returned by the `repos/{owner}/{repo}/branches` API.
#[derive(Debug, Clone, Deserialize)]
pub struct Branch {
    pub name: String,
    /// Tip commit SHA.
    #[serde(default)]
    pub sha: String,
    #[serde(default)]
    pub protected: bool,
}
