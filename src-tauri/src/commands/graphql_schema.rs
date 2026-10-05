use rocket_app::graphql_schema::{FetchGraphQlSchemaInput, GraphQlSchemaCache, GraphQlSchemaDto};
use rocket_app::RequestExecutionService;
use rocket_shared::error::DomainError;
use tauri::State;

/// Fetches the endpoint's schema by introspection, or returns the cached one.
#[tauri::command]
pub async fn fetch_graphql_schema(
    input: FetchGraphQlSchemaInput,
    svc: State<'_, RequestExecutionService>,
    cache: State<'_, GraphQlSchemaCache>,
) -> Result<GraphQlSchemaDto, DomainError> {
    svc.fetch_graphql_schema(&cache, input).await
}

/// Returns the cached schema for an endpoint without touching the network.
#[tauri::command]
pub fn get_cached_graphql_schema(
    collection: Option<String>,
    environment_name: Option<String>,
    url: String,
    cache: State<'_, GraphQlSchemaCache>,
) -> Option<GraphQlSchemaDto> {
    cache.get(&GraphQlSchemaCache::key(
        collection.as_deref(),
        environment_name.as_deref(),
        &url,
    ))
}

/// Forgets the cached schema for an endpoint.
#[tauri::command]
pub fn clear_graphql_schema(
    collection: Option<String>,
    environment_name: Option<String>,
    url: String,
    cache: State<'_, GraphQlSchemaCache>,
) {
    cache.clear(&GraphQlSchemaCache::key(
        collection.as_deref(),
        environment_name.as_deref(),
        &url,
    ));
}
