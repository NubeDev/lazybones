//! Seed one neutral default brand profile, idempotently.
//!
//! Branding is install-wide and any feature can reference a brand by id, so there
//! must always be at least one to pick. A brand-new install gets a single neutral
//! `default` brand; an existing one is left exactly as the operator left it.
//! Seeding **only creates the id if it is missing** — an operator's edits (or
//! deletion) are never clobbered on restart. Mirrors
//! [`seed_default_skills`](crate::seed_default_skills).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::Result;

use super::create::create_branding;
use super::get::get_branding;
use super::model::{BrandColors, BrandFonts, Branding};

/// The id of the bundled neutral default brand.
const DEFAULT_BRANDING_ID: &str = "default";

/// Insert the neutral default brand if it is missing. Returns how many brands
/// were newly created (0 once seeded and not wiped).
///
/// `now` stamps `created_at`/`updated_at` on the freshly seeded row.
///
/// # Errors
/// Returns [`StoreError::Operation`](crate::StoreError::Operation) on a
/// read/write failure.
pub async fn seed_default_branding(db: &Surreal<Db>, now: &str) -> Result<usize> {
    if get_branding(db, DEFAULT_BRANDING_ID).await?.is_some() {
        return Ok(0); // operator owns this id now — never overwrite.
    }

    let brand = Branding::new(DEFAULT_BRANDING_ID, "Default", now.to_owned())
        .with_colors(BrandColors {
            primary: "#1f2937".into(),
            secondary: "#4b5563".into(),
            accent: "#2563eb".into(),
            text: "#111827".into(),
            background: "#ffffff".into(),
        })
        .with_fonts(BrandFonts {
            heading: "Helvetica".into(),
            body: "Helvetica".into(),
        });
    create_branding(db, &brand).await?;
    Ok(1)
}
