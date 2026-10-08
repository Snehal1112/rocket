import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type { FlowTab } from '@/types/pane-types';

const clip = vi.hoisted(() => ({ copyTextAsync: vi.fn() }));
const files = vi.hoisted(() => ({ saveTextFile: vi.fn() }));
vi.mock('@/lib/clipboard', () => clip);
vi.mock('@/lib/save-file', () => files);
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));

import { FlowExportMenu } from '../FlowExportMenu';

// Radix menus call pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

// One distinct value per place a credential can sit. None may reach an export.
const FLOW_CANARIES = [
  'canary-auth-token-01',
  'canary-input-secret-02',
  'canary-inline-header-03',
  'canary-inline-url-04',
  'canary-inline-body-05',
];
const REPORT_CANARIES = [
  'canary-step-error-06',
  'canary-exchange-header-07',
  'canary-exchange-url-08',
  'canary-request-body-09',
  'canary-response-body-10',
  'canary-response-cookie-11',
  'canary-log-line-12',
  'canary-memory-token-13',
];

const node = (id: string, kind: FlowNode['kind']): FlowNode => ({
  id,
  kind,
  position: { x: 0, y: 0 },
});

const tab: FlowTab = {
  id: 't1',
  title: 'Flow: leaky',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'leaky',
  runId: 'run-1',
  runState: 'done',
  nodes: [
    node('a1', {
      kind: 'Auth',
      label: 'Sign in',
      auth: { authType: 'bearer', token: FLOW_CANARIES[0] },
      applyToInherit: true,
    }),
    node('in1', { kind: 'Input', label: 'Client secret', value: FLOW_CANARIES[1] }),
    node('rq1', {
      kind: 'Request',
      label: 'Items',
      source: {
        type: 'Inline',
        request: {
          method: 'POST',
          url: `https://api.test/items?api_key=${FLOW_CANARIES[3]}`,
          headers: [{ name: 'Authorization', value: `Bearer ${FLOW_CANARIES[2]}` }],
          body: `{"password":"${FLOW_CANARIES[4]}"}`,
        },
      },
    }),
  ],
  edges: [],
  nodeStatus: { a1: 'success', in1: 'success', rq1: 'failed' },
  nodeDetail: {
    a1: {},
    in1: {},
    rq1: {
      statusCode: 401,
      error: `Request failed: access_token=${REPORT_CANARIES[0]} was rejected`,
      exchange: {
        method: 'POST',
        url: `https://api.test/items?api_key=${REPORT_CANARIES[2]}`,
        headers: [{ key: 'Authorization', value: `Bearer ${REPORT_CANARIES[1]}` }],
        body: `{"password":"${REPORT_CANARIES[3]}","visible":"request-body-marker"}`,
        response: {
          status: 401,
          statusText: 'Unauthorized',
          durationMs: 10,
          sizeBytes: 40,
          headers: [{ key: 'Set-Cookie', value: `sid=${REPORT_CANARIES[5]}` }],
          body: `{"access_token":"${REPORT_CANARIES[4]}","visible":"response-body-marker"}`,
        },
      },
      logs: [{ level: 'log', message: `sent with token=${REPORT_CANARIES[6]}` }],
    },
  },
};

async function pick(name: string) {
  await userEvent.click(screen.getByRole('button', { name: 'Export' }));
  await userEvent.click(await screen.findByRole('menuitem', { name }));
}

function savedTexts(): string[] {
  return files.saveTextFile.mock.calls.map((call) => call[1] as string);
}

describe('FlowExportMenu never exports a credential', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clip.copyTextAsync.mockResolvedValue(undefined);
    files.saveTextFile.mockResolvedValue(true);
    // A token held in memory must stay out of every export.
    useFlowAuthStore.setState({
      auths: {
        'demo::leaky::::::a1': {
          auth: {
            authType: 'oauth2',
            oauth2: { accessToken: REPORT_CANARIES[7] } as never,
          },
        },
      },
    });
  });

  it('keeps every flow canary out of the copied JSON', async () => {
    render(<FlowExportMenu tab={tab} />);
    await pick('Copy flow JSON');
    const text = await (clip.copyTextAsync.mock.calls[0][0] as Promise<string>);
    for (const canary of [...FLOW_CANARIES, ...REPORT_CANARIES]) {
      expect(text).not.toContain(canary);
    }
  });

  it('keeps every canary out of both report formats, with bodies off', async () => {
    render(<FlowExportMenu tab={tab} />);
    await pick('Export run report (JSON)');
    await pick('Export run report (Markdown)');
    expect(savedTexts()).toHaveLength(2);
    for (const text of savedTexts()) {
      for (const canary of [...FLOW_CANARIES, ...REPORT_CANARIES]) {
        expect(text).not.toContain(canary);
      }
      expect(text).not.toContain('request-body-marker');
      expect(text).not.toContain('response-body-marker');
    }
  });

  it('keeps every canary out of both report formats, with bodies on, and shows the bodies', async () => {
    render(<FlowExportMenu tab={tab} />);
    await userEvent.click(screen.getByRole('button', { name: 'Export' }));
    await userEvent.click(
      await screen.findByRole('menuitemcheckbox', { name: 'Include bodies in run report' }),
    );
    await userEvent.click(screen.getByRole('menuitem', { name: 'Export run report (JSON)' }));
    await pick('Export run report (Markdown)');

    expect(savedTexts()).toHaveLength(2);
    for (const text of savedTexts()) {
      for (const canary of [...FLOW_CANARIES, ...REPORT_CANARIES]) {
        expect(text).not.toContain(canary);
      }
      expect(text).toContain('request-body-marker');
      expect(text).toContain('response-body-marker');
    }
  });
});
