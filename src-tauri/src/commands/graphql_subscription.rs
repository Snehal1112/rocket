use rocket_app::{
    resolve_graphql_subscription, GraphQlSubscribeInput, GraphQlSubscriptionService,
    RequestExecutionService,
};
use rocket_shared::error::DomainError;
use tauri::State;

/// Starts a GraphQL subscription under a caller-chosen session id. Results and status changes
/// arrive as the `graphql:subscription-message` and `graphql:subscription-status` events; this
/// only reports whether the socket opened.
#[tauri::command]
pub async fn graphql_subscribe(
    session_id: String,
    input: GraphQlSubscribeInput,
    exec: State<'_, RequestExecutionService>,
    subscriptions: State<'_, GraphQlSubscriptionService>,
) -> Result<(), DomainError> {
    let start = resolve_graphql_subscription(&exec, input).await?;
    subscriptions.start(&session_id, start).await
}

#[tauri::command]
pub async fn graphql_unsubscribe(
    session_id: String,
    subscriptions: State<'_, GraphQlSubscriptionService>,
) -> Result<(), DomainError> {
    subscriptions.stop(&session_id).await
}
