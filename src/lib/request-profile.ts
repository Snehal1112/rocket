import type { HttpMethod, RequestState } from '@/types/pane-types';

export interface RequestProfile {
  methods: HttpMethod[];
  bodyTabLabel: 'Body' | 'Query';
  showLoadTest: boolean;
  showCopyAsCurl: boolean;
  initialSection: 'params' | 'body';
  showSchema: boolean;
}

const HTTP_METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'HEAD'];

// What the request panel offers for each protocol. Load test and copy-as-cURL
// read the HTTP body, which a GraphQL tab does not have.
export function requestProfile(kind: RequestState['requestType']): RequestProfile {
  if (kind === 'graphql') {
    return {
      methods: ['POST', 'GET'],
      bodyTabLabel: 'Query',
      showLoadTest: false,
      showCopyAsCurl: false,
      initialSection: 'body',
      showSchema: true,
    };
  }
  return {
    methods: HTTP_METHODS,
    bodyTabLabel: 'Body',
    showLoadTest: true,
    showCopyAsCurl: true,
    initialSection: 'params',
    showSchema: false,
  };
}
