import {
  getActiveGlobalEnvName,
  getActiveWorkspaceRequestGuardPolicy,
  resolveRequestFieldsForPath,
  toApiOptions,
} from '@/lib/execute-request';
import { parseGraphQlResponse } from '@/lib/graphql-response';
import { mapApiRequestToState, mapGraphQlToState } from '@/lib/pane-utils';
import {
  type ExecuteRequestInput,
  type ExecuteRequestResponse,
  executeGraphQlRequest,
  executeRequest,
  type GraphQlRequest,
  type Request,
} from '@/lib/tauri-api';

export interface RunnerExecutionOutcome {
  status: 'passed' | 'failed';
  result?: ExecuteRequestResponse;
  error?: string;
}

// Executes one request from a collection tree walk (not an open tab),
// applying the same variable resolution and inherit-auth semantics a
// sidebar-opened tab gets, via mapApiRequestToState(request, true).
export async function executeRunnerEntry(
  collection: string,
  requestPath: string,
  request: Request,
  environmentName: string | undefined,
  graphql?: GraphQlRequest,
): Promise<RunnerExecutionOutcome> {
  try {
    const requestState = graphql ? mapGraphQlToState(graphql) : mapApiRequestToState(request, true);
    const resolved = await resolveRequestFieldsForPath(collection, requestPath, requestState);
    const globalEnvName = getActiveGlobalEnvName();
    const requestGuardPolicy = await getActiveWorkspaceRequestGuardPolicy();

    const input: ExecuteRequestInput = {
      method: request.method,
      url: resolved.url,
      headers: resolved.headers,
      queryParams: resolved.queryParams,
      body: resolved.body,
      auth: resolved.auth,
      options: toApiOptions(requestState.settings),
      environmentName,
      collection,
      requestName: request.name,
      requestPath,
      preRequestScript: request.preRequestScript ?? undefined,
      postResponseScript: request.postResponseScript ?? undefined,
      testsScript: request.tests ?? undefined,
      assertions: resolved.assertions,
      tags: request.tags,
      actions: request.actions,
      globalEnvName,
      pathParams: resolved.pathParams,
      requestGuardPolicy,
    };

    const result = graphql
      ? await executeGraphQlRequest({
          request: { ...input, body: undefined },
          query: resolved.graphql?.query ?? graphql.body.query,
          variables: resolved.graphql?.variables ?? (graphql.body.variables || undefined),
          operationName: undefined,
        })
      : await executeRequest(input);
    const hasFailingTest = result.testResults.some((t) => t.status === 'failed');
    const isErrorStatus = result.status < 200 || result.status >= 300;
    const hasGraphQlErrors = graphql ? parseGraphQlResponse(result.body).errors.length > 0 : false;
    const failed =
      isErrorStatus || hasFailingTest || Boolean(result.scriptError) || hasGraphQlErrors;
    return { status: failed ? 'failed' : 'passed', result };
  } catch (err) {
    return { status: 'failed', error: err instanceof Error ? err.message : String(err) };
  }
}
