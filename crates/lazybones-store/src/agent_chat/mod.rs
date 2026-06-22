//! Durable management-agent conversations: the model, persisted rows, and the
//! create/list/append/history verbs (`docs/agent/lazybones-agent-scope.md` §8.3).

mod model;
mod row;
mod store;

pub use model::{AgentConversation, AgentMessage, AgentRole, ConfirmAction};
pub use store::{
    agent_message_history, append_agent_message, append_confirm_request,
    create_agent_conversation, get_agent_conversation, list_agent_conversations,
};

#[cfg(test)]
mod tests {
    use crate::bootstrap::use_namespace;
    use crate::connect::{StoreEngine, open_engine};
    use crate::init_schema::init_schema;

    use super::*;

    async fn db() -> surrealdb::Surreal<surrealdb::engine::local::Db> {
        let db = open_engine(&StoreEngine::Memory).await.unwrap();
        use_namespace(&db, "lazybones", "test").await.unwrap();
        init_schema(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn conversation_and_messages_roundtrip() {
        let db = db().await;
        let ctx = serde_json::json!({ "view": "workflows", "workflow_id": "wf-1" });
        let conv = create_agent_conversation(&db, Some(&ctx), "2026-06-21T00:00:00Z")
            .await
            .unwrap();
        assert!(!conv.id.is_empty());
        assert_eq!(conv.page_context, Some(ctx));

        let got = get_agent_conversation(&db, &conv.id).await.unwrap().unwrap();
        assert_eq!(got.id, conv.id);

        append_agent_message(&db, &conv.id, AgentRole::User, "hi", None)
            .await
            .unwrap();
        append_agent_message(&db, &conv.id, AgentRole::Agent, "hello", Some(3))
            .await
            .unwrap();

        let hist = agent_message_history(&db, &conv.id).await.unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].role, AgentRole::User);
        assert_eq!(hist[1].role, AgentRole::Agent);

        let all = list_agent_conversations(&db).await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn confirm_request_persists_action_and_replays() {
        let db = db().await;
        let conv = create_agent_conversation(&db, None, "2026-06-21T00:00:00Z")
            .await
            .unwrap();
        let action = ConfirmAction {
            action: "start".into(),
            method: "POST".into(),
            path: "/workflows/wf-1/start".into(),
            body: None,
        };
        let msg = append_confirm_request(&db, &conv.id, "Start workflow wf-1?", &action, Some(9))
            .await
            .unwrap();
        assert_eq!(msg.role, AgentRole::Confirm);
        assert_eq!(msg.action.as_ref().unwrap().path, "/workflows/wf-1/start");

        // Dedups on hcom_id like a mirrored reply.
        append_confirm_request(&db, &conv.id, "Start workflow wf-1?", &action, Some(9))
            .await
            .unwrap();

        // History replays the confirm with its action intact.
        let hist = agent_message_history(&db, &conv.id).await.unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].role, AgentRole::Confirm);
        assert_eq!(hist[0].action.as_ref().unwrap().action, "start");
    }

    #[tokio::test]
    async fn agent_replies_dedup_on_hcom_id() {
        let db = db().await;
        let conv = create_agent_conversation(&db, None, "2026-06-21T00:00:00Z")
            .await
            .unwrap();
        let a = append_agent_message(&db, &conv.id, AgentRole::Agent, "r", Some(7))
            .await
            .unwrap();
        let b = append_agent_message(&db, &conv.id, AgentRole::Agent, "r", Some(7))
            .await
            .unwrap();
        assert_eq!(a, b);
        let hist = agent_message_history(&db, &conv.id).await.unwrap();
        assert_eq!(hist.len(), 1);
    }
}
