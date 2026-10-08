import { beforeEach, describe, expect, it } from 'vitest';
import type { Flow, FlowNode } from '@/lib/tauri-api';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type { FlowTab } from '@/types/pane-types';
import {
  buildRunReport,
  EXPORT_BODY_LIMIT,
  exportableFlow,
  maskFlowSecrets,
  REDACTED,
  redactKnownSecrets,
  reportFileName,
} from '../flow-export';

const node = (id: string, kind: FlowNode['kind']): FlowNode => ({
  id,
  kind,
  position: { x: 0, y: 0 },
});

describe('redactKnownSecrets', () => {
  it('redacts an Authorization header line', () => {
    expect(redactKnownSecrets('Authorization: Bearer abc.def.ghi')).toBe(
      `Authorization: ${REDACTED}`,
    );
  });

  it.each(['cookie: a=b; c=d', 'Set-Cookie: sid=1', 'X-Api-Key: k-123456', 'x-auth-token: t1'])(
    'redacts the value of %s',
    (line) => {
      const out = redactKnownSecrets(line);
      expect(out).toContain(REDACTED);
      expect(out.split(':')[0]).toBe(line.split(':')[0]);
    },
  );

  it('redacts a query parameter with a credential name and keeps the others', () => {
    expect(redactKnownSecrets('GET https://x.test/p?access_token=abc123&page=2')).toBe(
      `GET https://x.test/p?access_token=${REDACTED}&page=2`,
    );
    expect(redactKnownSecrets('client_secret=s3cr3t&grant_type=password')).toBe(
      `client_secret=${REDACTED}&grant_type=password`,
    );
  });

  it('redacts a JSON string value by key and keeps unrelated keys', () => {
    expect(redactKnownSecrets('{"access_token":"abc","token_type":"Bearer"}')).toBe(
      `{"access_token":"${REDACTED}","token_type":"Bearer"}`,
    );
    expect(redactKnownSecrets('{"user_token": "x", "name": "n"}')).toBe(
      `{"user_token": "${REDACTED}", "name": "n"}`,
    );
  });

  it('redacts a bearer credential that looks like a token', () => {
    expect(redactKnownSecrets('failed with Bearer eyJhbGci.payload.sig today')).toBe(
      `failed with Bearer ${REDACTED} today`,
    );
    expect(redactKnownSecrets('Bearer abc12345678')).toBe(`Bearer ${REDACTED}`);
  });

  it('leaves prose and variable references alone', () => {
    expect(redactKnownSecrets('Basic authentication required')).toBe(
      'Basic authentication required',
    );
    expect(redactKnownSecrets('/p?api_key={{key}}&x=1')).toBe('/p?api_key={{key}}&x=1');
    expect(redactKnownSecrets('{"password":"{{pw}}"}')).toBe('{"password":"{{pw}}"}');
    expect(redactKnownSecrets('/p?apikey=&x=1')).toBe('/p?apikey=&x=1');
  });

  it('is idempotent', () => {
    const once = redactKnownSecrets('Authorization: Bearer a1.b2 ?token=zz9 {"password":"p"}');
    expect(redactKnownSecrets(once)).toBe(once);
  });
});

describe('redactKnownSecrets scheme words, URL passwords and key names', () => {
  it('keeps a scheme followed only by references', () => {
    expect(redactKnownSecrets('Authorization: Bearer {{token}}')).toBe(
      'Authorization: Bearer {{token}}',
    );
    expect(redactKnownSecrets('Authorization: Basic {{creds}}')).toBe(
      'Authorization: Basic {{creds}}',
    );
    expect(redactKnownSecrets('Authorization: Bearer')).toBe('Authorization: Bearer');
  });

  it('still redacts a literal after a scheme word', () => {
    expect(redactKnownSecrets('Authorization: Basic dXNlcjpwdw==')).toBe(
      `Authorization: ${REDACTED}`,
    );
    expect(redactKnownSecrets('Authorization: Token abc')).toBe(`Authorization: ${REDACTED}`);
  });

  it('redacts a password in a URL and keeps the user and host', () => {
    expect(redactKnownSecrets('GET https://u:PWURL123@h.test/x?a=1')).toBe(
      `GET https://u:${REDACTED}@h.test/x?a=1`,
    );
  });

  it('keeps a URL password that is only references, and a URL without one', () => {
    expect(redactKnownSecrets('https://u:{{pw}}@h.test/x')).toBe('https://u:{{pw}}@h.test/x');
    expect(redactKnownSecrets('https://h.test:8080/x')).toBe('https://h.test:8080/x');
    expect(redactKnownSecrets('https://u@h.test/x')).toBe('https://u@h.test/x');
  });

  it('is idempotent for URL passwords and scheme words', () => {
    const once = redactKnownSecrets('https://u:pw1@h.test Authorization: Basic {{c}}');
    expect(redactKnownSecrets(once)).toBe(once);
  });

  it.each([
    'private_key',
    'client_assertion',
    'pass',
    'pwd',
    'secret_access_key',
    'x-amz-signature',
    'signature',
  ])('redacts a parameter named %s', (name) => {
    expect(redactKnownSecrets(`/p?${name}=abc123&x=1`)).toBe(`/p?${name}=${REDACTED}&x=1`);
  });

  it('keeps look-alike names that are not credentials', () => {
    expect(redactKnownSecrets('/p?token_type=Bearer&compass=1&x=1')).toBe(
      '/p?token_type=Bearer&compass=1&x=1',
    );
  });
});

describe('exportableFlow and maskFlowSecrets', () => {
  const flow: Flow = {
    name: 'login',
    callbackHost: '10.0.0.5',
    nodes: [
      node('auth1', {
        kind: 'Auth',
        label: 'Sign in',
        auth: { authType: 'bearer', token: 'canary-bearer-7c1d' },
        applyToInherit: true,
      }),
      node('auth2', {
        kind: 'Auth',
        label: 'Ref',
        auth: { authType: 'bearer', token: '{{token}}' },
        applyToInherit: false,
      }),
      node('in1', { kind: 'Input', label: 'API token', value: 'canary-input-2b9e' }),
      node('in2', { kind: 'Input', label: 'Username', value: 'alice' }),
      node('in3', { kind: 'Input', label: 'Password', value: '{{pw}}' }),
      node('in4', { kind: 'Input', label: 'Secret blob', value: { a: 1 } }),
      node('rq1', {
        kind: 'Request',
        label: 'Items',
        source: {
          type: 'Inline',
          request: {
            method: 'POST',
            url: 'https://api.test/items?api_key=canary-url-33&page=1',
            headers: [
              { name: 'Authorization', value: 'Bearer canary-header-5a44' },
              { name: 'X-Ref', value: '{{ref}}' },
              { name: 'X-Api-Key', value: '{{key}}' },
              { name: 'X-Trace', value: 'on' },
            ],
            body: '{"password":"canary-body-91","name":"n"}',
          },
        },
      }),
      node('rq2', {
        kind: 'Request',
        label: 'Saved',
        source: { type: 'Saved', requestPath: 'a/b.yml' },
      }),
      node('out1', { kind: 'Output', label: 'Out' }),
    ],
    edges: [
      {
        id: 'e1',
        sourceNodeId: 'auth1',
        targetNodeId: 'rq1',
        targetField: 'auth',
        expression: '',
      },
    ],
  };

  const asTab = (over: Partial<FlowTab> = {}): FlowTab => ({
    id: 't1',
    title: 'Flow: login',
    isDirty: false,
    tabType: 'flow',
    collectionName: 'demo',
    flowName: 'login',
    nodes: flow.nodes,
    edges: flow.edges,
    nodeStatus: {},
    runState: 'idle',
    callbackHost: '10.0.0.5',
    ...over,
  });

  it('builds the flow from a tab, with the callback host only when set', () => {
    expect(exportableFlow(asTab())).toEqual(flow);
    expect(exportableFlow(asTab({ callbackHost: null }))).not.toHaveProperty('callbackHost');
    expect(exportableFlow(asTab({ flowName: null }))).toBeNull();
  });

  it('masks literal credentials and counts them', () => {
    const out = maskFlowSecrets(flow);
    const text = JSON.stringify(out.flow);
    for (const canary of [
      'canary-bearer-7c1d',
      'canary-input-2b9e',
      'canary-url-33',
      'canary-header-5a44',
      'canary-body-91',
    ]) {
      expect(text).not.toContain(canary);
    }
    // Auth token, Input value, request URL, Authorization header, request body.
    expect(out.maskedCount).toBe(5);
  });

  it('keeps references, plain values, structured values and everything else', () => {
    const out = maskFlowSecrets(flow);
    const byId = (id: string) => out.flow.nodes.find((n) => n.id === id)?.kind;
    expect(byId('auth2')).toEqual(flow.nodes[1].kind);
    expect(byId('in2')).toEqual(flow.nodes[3].kind);
    expect(byId('in3')).toEqual(flow.nodes[4].kind);
    expect(byId('in4')).toEqual(flow.nodes[5].kind);
    expect(byId('rq2')).toEqual(flow.nodes[7].kind);
    expect(out.flow.edges).toEqual(flow.edges);
    expect(out.flow.callbackHost).toBe('10.0.0.5');
    expect(out.flow.name).toBe('login');
    const inline = byId('rq1');
    if (inline?.kind !== 'Request' || inline.source.type !== 'Inline') throw new Error('shape');
    const headers = inline.source.request.headers;
    expect(headers[0]).toEqual({ name: 'Authorization', value: REDACTED });
    expect(headers[1]).toEqual({ name: 'X-Ref', value: '{{ref}}' });
    expect(headers[2]).toEqual({ name: 'X-Api-Key', value: '{{key}}' });
    expect(headers[3]).toEqual({ name: 'X-Trace', value: 'on' });
    expect(inline.source.request.url).toBe(`https://api.test/items?api_key=${REDACTED}&page=1`);
    expect(inline.source.request.body).toBe(`{"password":"${REDACTED}","name":"n"}`);
  });

  it('does not mask or count a scheme word followed only by a reference', () => {
    const f: Flow = {
      name: 'x',
      edges: [],
      nodes: [
        node('rq', {
          kind: 'Request',
          label: 'R',
          source: {
            type: 'Inline',
            request: {
              method: 'GET',
              url: 'https://u:{{pw}}@h.test/x',
              headers: [{ name: 'Authorization', value: 'Bearer {{token}}' }],
            },
          },
        }),
      ],
    };
    const out = maskFlowSecrets(f);
    expect(out.maskedCount).toBe(0);
    expect(out.flow).toEqual(f);
  });

  it('masks a password in an inline request URL and counts it', () => {
    const f: Flow = {
      name: 'x',
      edges: [],
      nodes: [
        node('rq', {
          kind: 'Request',
          label: 'R',
          source: {
            type: 'Inline',
            request: { method: 'GET', url: 'https://u:PWURL123@h.test/x', headers: [] },
          },
        }),
      ],
    };
    const out = maskFlowSecrets(f);
    expect(JSON.stringify(out.flow)).not.toContain('PWURL123');
    expect(out.maskedCount).toBe(1);
  });

  it('does not change its input and reports zero for a clean flow', () => {
    const before = JSON.stringify(flow);
    maskFlowSecrets(flow);
    expect(JSON.stringify(flow)).toBe(before);
    expect(maskFlowSecrets({ name: 'x', nodes: [], edges: [] }).maskedCount).toBe(0);
  });
});

describe('buildRunReport', () => {
  const CANARIES = [
    'canary-err-12',
    'canary-exch-77',
    'canary-url-33',
    'canary-reqbody-5',
    'canary-cookie-8',
    'canary-resp-3',
    'canary-log-41',
    'canary-store-99',
  ];

  const tab = (over: Partial<FlowTab> = {}): FlowTab => ({
    id: 't1',
    title: 'Flow: login',
    isDirty: false,
    tabType: 'flow',
    collectionName: 'demo',
    flowName: 'login',
    runId: 'run-1',
    runState: 'done',
    nodes: [
      node('in1', { kind: 'Input', label: 'User', value: 'alice' }),
      node('rq1', {
        kind: 'Request',
        label: 'Items',
        source: { type: 'Saved', requestPath: 'a/items.yml' },
      }),
      node('rq2', {
        kind: 'Request',
        label: 'After',
        source: { type: 'Saved', requestPath: 'a/after.yml' },
      }),
      node('out1', { kind: 'Output', label: 'Out' }),
    ],
    edges: [],
    nodeStatus: { in1: 'success', rq1: 'failed', rq2: 'skipped', out1: 'skipped' },
    nodeDetail: {
      in1: { value: 'alice' },
      rq1: {
        statusCode: 401,
        durationMs: 120,
        attempts: 2,
        error: 'Request failed: access_token=canary-err-12 was rejected',
        exchange: {
          method: 'POST',
          url: 'https://api.test/items?api_key=canary-url-33&page=1',
          headers: [
            { key: 'Authorization', value: 'Bearer canary-exch-77' },
            { key: 'X-Trace', value: 'on' },
          ],
          body: '{"password":"canary-reqbody-5","note":"keep"}',
          response: {
            status: 401,
            statusText: 'Unauthorized',
            durationMs: 118,
            sizeBytes: 50,
            headers: [{ key: 'Set-Cookie', value: 'sid=canary-cookie-8' }],
            body: '{"access_token":"canary-resp-3","message":"nope"}',
          },
        },
        logs: [{ level: 'log', message: 'sent with token=canary-log-41' }],
      },
      rq2: { skipReason: 'upstream_failed' },
      out1: { skipReason: 'upstream_failed' },
    },
    ...over,
  });

  const fixedNow = () => new Date('2026-10-08T12:00:00.000Z');

  beforeEach(() => {
    // An in-memory token must never reach a report.
    useFlowAuthStore.setState({
      auths: {
        k: {
          auth: {
            authType: 'oauth2',
            oauth2: { accessToken: 'canary-store-99' } as never,
          },
        },
      },
    });
  });

  it('redacts the run value of a sensitive Input node in both formats', () => {
    const t = tab({
      nodes: [node('in1', { kind: 'Input', label: 'API token', value: 'x' })],
      nodeStatus: { in1: 'success' },
      nodeDetail: { in1: { value: 'sk_live_INPUTVAL' } },
    });
    const out = buildRunReport(t, { includeBodies: false, now: fixedNow });
    expect(out.json).not.toContain('sk_live_INPUTVAL');
    expect(out.markdown).not.toContain('sk_live_INPUTVAL');
    expect(JSON.parse(out.json).nodes[0].value).toBe(REDACTED);
  });

  it('keeps the run value of a plain Input node and of an Output node', () => {
    const out = JSON.parse(buildRunReport(tab(), { includeBodies: false, now: fixedNow }).json);
    expect(out.nodes[0].value).toBe('alice');
  });

  it('redacts a password in an exchange URL', () => {
    const t = tab({
      nodeDetail: {
        rq1: {
          exchange: { method: 'GET', url: 'https://u:PWURL123@h.test/x', headers: [] },
        },
      },
    });
    const out = buildRunReport(t, { includeBodies: false, now: fixedNow });
    expect(out.json).not.toContain('PWURL123');
    expect(out.markdown).not.toContain('PWURL123');
    expect(out.json).toContain(`https://u:${REDACTED}@h.test/x`);
  });

  it('describes the run, with a summary, in canvas order', () => {
    const report = JSON.parse(buildRunReport(tab(), { includeBodies: false, now: fixedNow }).json);
    expect(report).toMatchObject({
      flow: 'login',
      collection: 'demo',
      runId: 'run-1',
      generatedAt: '2026-10-08T12:00:00.000Z',
      includeBodies: false,
      summary: { total: 4, success: 1, failed: 1, skipped: 2, notRun: 0 },
    });
    expect(report.nodes.map((n: { id: string }) => n.id)).toEqual(['in1', 'rq1', 'rq2', 'out1']);
    expect(report.nodes[1]).toMatchObject({
      label: 'Items',
      kind: 'Request',
      status: 'failed',
      statusCode: 401,
      durationMs: 120,
      attempts: 2,
    });
    expect(report.nodes[2]).toMatchObject({ status: 'skipped', skipReason: 'upstream_failed' });
  });

  it('leaves out the fields a node does not have', () => {
    const report = JSON.parse(buildRunReport(tab(), { includeBodies: false }).json);
    const output = report.nodes[3];
    for (const key of ['statusCode', 'durationMs', 'attempts', 'branch', 'error', 'value']) {
      expect(output).not.toHaveProperty(key);
    }
  });

  it('leaves out bodies by default and redacts every other free-text field', () => {
    const { json, markdown } = buildRunReport(tab(), { includeBodies: false, now: fixedNow });
    for (const text of [json, markdown]) {
      for (const canary of CANARIES) expect(text).not.toContain(canary);
      expect(text).not.toContain('keep');
      expect(text).not.toContain('nope');
      expect(text).toContain(REDACTED);
    }
    const exchange = JSON.parse(json).nodes[1].exchange;
    expect(exchange).not.toHaveProperty('body');
    expect(exchange.response).not.toHaveProperty('body');
    expect(exchange.headers).toEqual([
      { key: 'Authorization', value: REDACTED },
      { key: 'X-Trace', value: 'on' },
    ]);
  });

  it('includes bodies on request, redacted', () => {
    const { json, markdown } = buildRunReport(tab(), { includeBodies: true });
    for (const text of [json, markdown]) {
      for (const canary of CANARIES) expect(text).not.toContain(canary);
      expect(text).toContain('keep');
      expect(text).toContain('nope');
    }
    expect(JSON.parse(json).includeBodies).toBe(true);
  });

  it('cuts a very large body', () => {
    const big = 'x'.repeat(EXPORT_BODY_LIMIT + 5000);
    const base = tab();
    const detail = base.nodeDetail ?? {};
    const exchange = detail.rq1.exchange;
    if (!exchange?.response) throw new Error('fixture');
    const large = tab({
      nodeDetail: {
        ...detail,
        rq1: { ...detail.rq1, exchange: { ...exchange, response: { ...exchange.response, body: big } } },
      },
    });
    const { json } = buildRunReport(large, { includeBodies: true });
    const body = JSON.parse(json).nodes[1].exchange.response.body as string;
    expect(body.length).toBeLessThan(EXPORT_BODY_LIMIT + 200);
    expect(body).toContain('cut at');
  });

  it('renders Markdown with a section per node and safe fences', () => {
    const base = tab();
    const withTicks = tab({
      nodeDetail: { ...base.nodeDetail, out1: { value: 'a ``` b' } },
      nodeStatus: { ...base.nodeStatus, out1: 'success' },
    });
    const { markdown } = buildRunReport(withTicks, { includeBodies: false, now: fixedNow });
    expect(markdown).toContain('# Run report: login');
    expect(markdown).toContain('- Generated: 2026-10-08T12:00:00.000Z');
    expect(markdown).toContain('## 2. Items (Request) - Failed');
    expect(markdown).toContain('- Status code: 401');
    expect(markdown).toContain('- Skipped: upstream failed');
    // The value holds three backticks, so its fence must be longer.
    expect(markdown).toContain('````\na ``` b\n````');
  });

  it('still builds a report when nothing ran', () => {
    const idle = tab({ runState: 'idle', nodeStatus: {}, nodeDetail: undefined, runId: undefined });
    const report = JSON.parse(buildRunReport(idle, { includeBodies: false }).json);
    expect(report.summary).toMatchObject({ total: 4, success: 0, failed: 0, skipped: 0, notRun: 4 });
    expect(report).not.toHaveProperty('runId');
  });
});

describe('reportFileName', () => {
  it('makes a safe file name', () => {
    expect(reportFileName('My Flow/1', 'json')).toBe('My-Flow-1-run-report.json');
    expect(reportFileName('login', 'md')).toBe('login-run-report.md');
    expect(reportFileName('///', 'md')).toBe('flow-run-report.md');
  });
});
