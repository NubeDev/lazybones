//! Seed the agent catalog with the bundled 2026 defaults, idempotently.
//!
//! The defaults ship in `agents.default.yaml` (compiled in via `include_str!`),
//! so the model/effort menus are editable data, not code. Seeding **only creates
//! agents that don't already exist** — an operator's edits and deletions are
//! never clobbered on restart. A brand-new install gets a usable catalog; an
//! existing one is left exactly as the operator left it.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::create::create_agent;
use super::get::get_agent;
use super::model::AgentCatalog;

/// The bundled default catalog, as authored YAML (a list of `AgentCatalog`).
const DEFAULT_AGENTS_YAML: &str = include_str!("agents.default.yaml");

/// Insert any bundled default agents missing from the store. Returns how many
/// were newly created (0 once the catalog has been seeded and not wiped).
///
/// `now` stamps `created_at`/`updated_at` on freshly seeded rows.
///
/// # Errors
/// Returns [`StoreError::AgentSeed`] if the bundled YAML can't be parsed, or
/// [`StoreError::Operation`] on a read/write failure.
pub async fn seed_default_agents(db: &Surreal<Db>, now: &str) -> Result<usize> {
    let defaults: Vec<SeedAgent> = serde_yaml::from_str(DEFAULT_AGENTS_YAML)
        .map_err(|e| StoreError::AgentSeed(format!("agents.default.yaml: {e}")))?;

    let mut created = 0;
    for seed in defaults {
        if get_agent(db, &seed.id).await?.is_some() {
            continue; // operator owns this id now — never overwrite.
        }
        create_agent(db, &seed.into_agent(now)).await?;
        created += 1;
    }
    Ok(created)
}

/// One agent as authored in the bundled YAML (no timestamps — seeding stamps).
#[derive(Debug, serde::Deserialize)]
struct SeedAgent {
    id: String,
    label: String,
    env_var: String,
    #[serde(default)]
    login_hint: String,
    #[serde(default)]
    models: Vec<String>,
    #[serde(default)]
    default_model: Option<String>,
    #[serde(default)]
    efforts: Vec<String>,
    #[serde(default)]
    default_effort: Option<String>,
}

impl SeedAgent {
    fn into_agent(self, now: &str) -> AgentCatalog {
        AgentCatalog::new(
            self.id,
            self.label,
            self.env_var,
            self.login_hint,
            self.models,
            self.default_model,
            self.efforts,
            self.default_effort,
            now.to_owned(),
        )
    }
}
