//! Seed the skill catalogue with a few bundled demo skills, idempotently.
//!
//! The demos ship in `skills.default.yaml` (compiled in via `include_str!`), so
//! the starter recipes are editable data, not code. Seeding **only creates skills
//! that don't already exist** — an operator's edits and deletions are never
//! clobbered on restart. A brand-new install gets a handful of usable demo skills
//! to attach to templates; an existing one is left exactly as the operator left
//! it. Mirrors [`seed_default_agents`](crate::seed_default_agents).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::create::create_skill;
use super::get::get_skill;
use super::model::Skill;

/// The bundled demo catalogue, as authored YAML (a list of `SeedSkill`).
const DEFAULT_SKILLS_YAML: &str = include_str!("skills.default.yaml");

/// Insert any bundled demo skills missing from the store. Returns how many were
/// newly created (0 once the catalogue has been seeded and not wiped).
///
/// `now` stamps `created_at`/`updated_at` on freshly seeded rows.
///
/// # Errors
/// Returns [`StoreError::SkillSeed`] if the bundled YAML can't be parsed, or
/// [`StoreError::Operation`] on a read/write failure.
pub async fn seed_default_skills(db: &Surreal<Db>, now: &str) -> Result<usize> {
    let defaults: Vec<SeedSkill> = serde_yaml::from_str(DEFAULT_SKILLS_YAML)
        .map_err(|e| StoreError::SkillSeed(format!("skills.default.yaml: {e}")))?;

    let mut created = 0;
    for seed in defaults {
        if get_skill(db, &seed.id).await?.is_some() {
            continue; // operator owns this id now — never overwrite.
        }
        create_skill(db, &seed.into_skill(now)).await?;
        created += 1;
    }
    Ok(created)
}

/// One skill as authored in the bundled YAML (no timestamps — seeding stamps).
#[derive(Debug, serde::Deserialize)]
struct SeedSkill {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    body: String,
    /// An optional structured action (open question 2); omitted for plain
    /// markdown-runbook skills.
    #[serde(default)]
    action: Option<super::model::SkillAction>,
}

impl SeedSkill {
    fn into_skill(self, now: &str) -> Skill {
        let skill = Skill::new(self.id, self.title, self.description, self.body, now.to_owned());
        match self.action {
            Some(action) => skill.with_action(action),
            None => skill,
        }
    }
}
