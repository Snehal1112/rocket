import { resolveRequestFieldsForPath } from '@/lib/execute-request';
import { mapApiRequestToState } from '@/lib/pane-utils';
import {
  type ExecuteRequestInput,
  type ExecuteRequestResponse,
  executeRequest,
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
): Promise<RunnerExecutionOutcome> {
  try {
    const requestState = mapApiRequestToState(request, true);
    const resolved = await resolveRequestFieldsForPath(collection, requestPath, requestState);

    const input: ExecuteRequestInput = {
      method: request.method,
      url: resolved.url,
      headers: resolved.headers,
      queryParams: resolved.queryParams,
      body: resolved.body,
      auth: resolved.auth,
      options: {
        followRedirects: requestState.settings.followRedirects,
        timeoutMs: requestState.settings.timeoutMs,
        verifySsl: requestState.settings.verifySsl,
      },
      environmentName,
      collection,
      requestName: request.name,
      requestPath,
      preRequestScript: request.preRequestScript ?? undefined,
      postResponseScript: request.postResponseScript ?? undefined,
      testsScript: request.tests ?? undefined,
      assertions: request.assertions,
      tags: request.tags,
      actions: request.actions,
    };

    const result = await executeRequest(input);
    const hasFailingTest = result.testResults.some((t) => t.status === 'failed');
    const isErrorStatus = result.status < 200 || result.status >= 300;
    const failed = isErrorStatus || hasFailingTest || Boolean(result.scriptError);
    return { status: failed ? 'failed' : 'passed', result };
  } catch (err) {
    return { status: 'failed', error: err instanceof Error ? err.message : String(err) };
  }
}
