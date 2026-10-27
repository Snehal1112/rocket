import { useOpenApiSyncGlobalEvents } from '@/lib/queries/openapi-sync-queries';

/** Mounts the app-wide OpenAPI sync listeners once. It renders nothing. */
export function OpenApiSyncNotifier() {
  useOpenApiSyncGlobalEvents();
  return null;
}
