import { toApiAuth, toApiBody } from '@/lib/execute-request';
import {
  type AssistantResponseChip,
  buildAssistantChipResource,
  maskAssistantResponse,
  type PromptResourceDto,
} from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab, ResponseState } from '@/types/pane-types';
import { findRequestTab } from './request-tabs';
import type { ReferenceItem } from './types';

/** Largest text one chip adds to a message, in UTF-8 bytes, marker included. */
export const RESOURCE_LIMIT_BYTES = 8 * 1024;

/**
 * Cuts `text` to `limit` UTF-8 bytes, marker included, without splitting a character.
 * The backend already caps chip text. This is a second line of defense.
 */
export function capText(text: string, limit = RESOURCE_LIMIT_BYTES): string {
  const encoder = new TextEncoder();
  const bytes = encoder.encode(text);
  if (bytes.length <= limit) return text;
  const marker = `\n[truncated: ${bytes.length} bytes cut to ${limit}]`;
  const keep = limit - encoder.encode(marker).length;
  // A cut inside a character decodes to U+FFFD, which is dropped.
  const head = new TextDecoder().decode(bytes.slice(0, keep)).replace(/\uFFFD+$/, '');
  return `${head}${marker}`;
}

/** The resource URI of a chip, such as `rocket://request/shop/orders/list.yml`. */
export function chipUri(chip: ReferenceItem): string {
  const segments = [chip.collection, ...(chip.path ? chip.path.split('/') : [])];
  return `rocket://${chip.kind}/${segments.map(encodeURIComponent).join('/')}`;
}

/** What the backend needs to mask a tab's last response. Nothing is masked here. */
function responsePayload(tab: RequestTab, response: ResponseState): AssistantResponseChip {
  const request = tab.request;
  return {
    method: tab.request.method,
    url: tab.request.url,
    status: response.status,
    statusText: response.statusText,
    durationMs: response.durationMs,
    sizeBytes: response.sizeBytes,
    headers: response.headers.map((header) => ({ key: header.key, value: header.value })),
    body: response.body,
    isBinary: response.isBinary ?? false,
    tests: (response.testResults ?? []).map((test) => ({
      name: test.name,
      passed: test.status === 'passed',
      error: test.error,
    })),
    request: {
      headers: request.headers.map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
      queryParams: request.queryParams.map((q) => ({
        key: q.key,
        value: q.value,
        enabled: q.enabled,
      })),
      body: toApiBody(request.body),
      auth: toApiAuth(request.auth),
    },
  };
}

async function loadChip(chip: ReferenceItem): Promise<PromptResourceDto> {
  const path = chip.path ?? '';
  if (chip.kind === 'last-response') {
    const tab = findRequestTab(usePaneStore.getState().root, chip.collection, path);
    if (!tab?.response) {
      return {
        uri: chipUri(chip),
        mimeType: 'text/plain',
        text: `No response is available for ${chip.label}.`,
      };
    }
    // The active environment belongs to the active collection only. For a tab of another
    // collection no environment is sent, and the backend resolves every environment.
    const { activeEnvId, activeCollection } = useEnvStore.getState();
    const environmentName = activeCollection === chip.collection ? activeEnvId : null;
    return maskAssistantResponse(
      chip.collection,
      path,
      responsePayload(tab, tab.response),
      environmentName ?? undefined,
    );
  }
  return buildAssistantChipResource(chip.kind, chip.collection, chip.path);
}

export type ChipLoad =
  | { ok: true; resource: PromptResourceDto }
  | { ok: false; chip: ReferenceItem };

/** Loads a chip's resource. Never rejects: a failure is reported as `{ ok: false }`. */
export async function tryChipResource(chip: ReferenceItem): Promise<ChipLoad> {
  try {
    const resource = await loadChip(chip);
    return { ok: true, resource: { ...resource, text: capText(resource.text) } };
  } catch {
    // The error text can echo request content, so it is not passed on.
    return { ok: false, chip };
  }
}

/**
 * Turns a chip into an embedded text resource. The text is built and masked by the backend
 * from the same masked views the agent's read tools use. Never rejects: a chip that cannot
 * load becomes a short notice that does not echo the error.
 */
export async function chipToResource(chip: ReferenceItem): Promise<PromptResourceDto> {
  const loaded = await tryChipResource(chip);
  if (loaded.ok) return loaded.resource;
  return {
    uri: chipUri(chip),
    mimeType: 'text/plain',
    text: `Rocket could not load this ${chip.kind}: ${chip.label}.`,
  };
}
