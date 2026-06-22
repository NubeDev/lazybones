//! Teams: the mid container of the team graph, its persisted row, the thin
//! create/get/list verbs, plus the two relation helpers a team anchors —
//! [`under`](under) (containment into the org, and the downward traversal) and
//! [`member_of`](member) (membership + per-team role). Mirrors the
//! [`skill`](crate::skill) module shape.

mod create;
mod get;
mod list;
mod member;
mod model;
mod row;
mod under;

pub use create::create_team;
pub use get::get_team;
pub use list::list_teams;
pub use member::{MemberRole, Membership, add_member, members_of};
pub use model::Team;
pub use under::{org_teams, place_team_under_org};

pub(crate) use under::relate_under;

#[cfg(test)]
mod tests {
    use crate::bootstrap::use_namespace;
    use crate::connect::{StoreEngine, open_engine};
    use crate::init_schema::init_schema;
    use crate::org::{Org, create_org};
    use crate::user::{User, create_user};

    use super::*;

    async fn db() -> surrealdb::Surreal<surrealdb::engine::local::Db> {
        let db = open_engine(&StoreEngine::Memory).await.unwrap();
        use_namespace(&db, "lazybones", "test").await.unwrap();
        init_schema(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn create_get_list_roundtrip() {
        let db = db().await;
        let created = create_team(&db, &Team::new("platform", "Platform", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        assert_eq!(created.id, "platform");

        let got = get_team(&db, "platform").await.unwrap().unwrap();
        assert_eq!(got, created);
        assert_eq!(list_teams(&db).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn containment_traversal_returns_child_teams() {
        let db = db().await;
        create_org(&db, &Org::new("nube", "Nube", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        create_team(&db, &Team::new("platform", "Platform", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        create_team(&db, &Team::new("growth", "Growth", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        // A team placed elsewhere must not show up under `nube`.
        create_org(&db, &Org::new("acme", "Acme", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        create_team(&db, &Team::new("ops", "Ops", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();

        place_team_under_org(&db, "platform", "nube").await.unwrap();
        place_team_under_org(&db, "growth", "nube").await.unwrap();
        place_team_under_org(&db, "ops", "acme").await.unwrap();

        let mut children: Vec<String> =
            org_teams(&db, "nube").await.unwrap().into_iter().map(|t| t.id).collect();
        children.sort();
        assert_eq!(children, vec!["growth".to_owned(), "platform".to_owned()]);
    }

    #[tokio::test]
    async fn place_under_missing_org_is_not_found() {
        let db = db().await;
        create_team(&db, &Team::new("platform", "Platform", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        let err = place_team_under_org(&db, "platform", "ghost").await.unwrap_err();
        assert!(matches!(err, crate::StoreError::OrgNotFound(_)));
    }

    #[tokio::test]
    async fn membership_carries_per_team_role() {
        let db = db().await;
        create_team(&db, &Team::new("platform", "Platform", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        create_user(&db, &User::new("ada", "Ada", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        create_user(&db, &User::new("bob", "Bob", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();

        add_member(&db, "ada", "platform", MemberRole::Manager).await.unwrap();
        add_member(&db, "bob", "platform", MemberRole::Member).await.unwrap();

        let mut members = members_of(&db, "platform").await.unwrap();
        members.sort_by(|a, b| a.user.cmp(&b.user));
        assert_eq!(members.len(), 2);
        assert_eq!(members[0], Membership { user: "ada".into(), role: MemberRole::Manager });
        assert_eq!(members[1], Membership { user: "bob".into(), role: MemberRole::Member });
    }

    #[tokio::test]
    async fn add_member_missing_team_is_not_found() {
        let db = db().await;
        create_user(&db, &User::new("ada", "Ada", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        let err = add_member(&db, "ada", "ghost", MemberRole::Member).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::TeamNotFound(_)));
    }
}
