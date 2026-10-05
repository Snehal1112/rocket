import {
  type ExecuteRequestInput,
  type ExecuteRequestResponse,
  executeGraphQlRequest,
  executeRequest,
} from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';

/** The query and variables after `{{variable}}` resolution. */
export interface ResolvedGraphQl {
  query: string;
  variables?: string;
}

// Sends through the command that matches the tab's protocol. A GraphQL tab must
// never reach executeRequest: its HTTP body is empty and the backend builds the real one.
export function dispatchSend(
  request: RequestState,
  input: ExecuteRequestInput,
  graphql: ResolvedGraphQl | undefined,
): Promise<ExecuteRequestResponse> {
  if (request.requestType !== 'graphql') return executeRequest(input);
  return executeGraphQlRequest({
    request: { ...input, body: undefined },
    query: graphql?.query ?? '',
    variables: graphql?.variables,
    operationName: request.graphql?.operationName,
  });
}
