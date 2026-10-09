import { REDACTED, redactKnownSecrets } from '@/lib/flow-export';
import { isVariableReference } from '@/lib/flow-secrets';
import { isSensitiveHeader } from '@/lib/sensitive-headers';
import {
  type CollectionSettings,
  type Environment,
  type FolderSettings,
  getCollectionSettings,
  getEnvironment,
  getFolderSettings,
  getRequest,
  type PromptResourceDto,
  type Request,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab, ResponseState } from '@/types/pane-types';
import { findRequestTab } from './request-tabs';
import type { ReferenceItem } from './types';

/** Largest text one chip adds to a message, in UTF-8 bytes, marker included. */
export const RESOURCE_LIMIT_BYTES = 8 * 1024;

// A header, parameter or variable name that holds a credential.
const CREDENTIAL_NAME =
  /token|secret|password|passwd|api[_-]?key|credential|cookie|authorization|session/i;
const FENCE = '`'.repeat(3);

interface Row {
  key: string;
  value: string;
  enabled: boolean;
}

interface VariableRow extends Row {
  secret: boolean;
}

/** A value that is safe to share. `{{variable}}` references are kept as they are. */
export function maskValue(name: string, value: string): string {
  if (value.trim() === '' || isVariableReference(value)) return value;
  if (isSensitiveHeader(name) || CREDENTIAL_NAME.test(name)) return REDACTED;
  return redactKnownSecrets(value);
}

/** Cuts `text` to `limit` UTF-8 bytes, marker included, without splitting a character. */
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

function rowsSection(title: string, rows: readonly Row[]): string[] {
  const shown = rows.filter((row) => row.enabled && row.key.trim() !== '');
  if (shown.length === 0) return [];
  return [`${title}:`, ...shown.map((row) => `  ${row.key}: ${maskValue(row.key, row.value)}`)];
}

function variablesSection(title: string, rows: readonly VariableRow[]): string[] {
  const shown = rows.filter((row) => row.enabled && row.key.trim() !== '');
  if (shown.length === 0) return [];
  return [
    `${title}:`,
    ...shown.map((row) =>
      row.secret
        ? `  ${row.key}: (secret, value not shared)`
        : `  ${row.key}: ${maskValue(row.key, row.value)}`,
    ),
  ];
}

function codeSection(title: string, code: string | null | undefined, language: string): string[] {
  if (!code || code.trim() === '') return [];
  return [`${title}:`, `${FENCE}${language}`, code, FENCE];
}

function authLine(authType: string | undefined): string[] {
  if (!authType || authType === 'none') return [];
  if (authType === 'inherit') return ['Auth: inherited from the folder or collection'];
  return [`Auth: ${authType} (credential values are not shared)`];
}

function bodyText(mode: string, content: string, formData: readonly Row[]): string {
  if (mode === 'none') return '';
  if (mode === 'binary') return '(binary file, not shared)';
  if (mode === 'formdata' || mode === 'formurlencoded') {
    return formData
      .filter((row) => row.enabled && row.key.trim() !== '')
      .map((row) => `${row.key}=${maskValue(row.key, row.value)}`)
      .join('\n');
  }
  return redactKnownSecrets(content);
}

/** The parts of a request a chip shows, from an open tab or from disk. */
interface RequestView {
  title: string;
  unsaved: boolean;
  kind: string;
  method: string;
  url: string;
  queryParams: readonly Row[];
  headers: readonly Row[];
  authType: string;
  bodyMode: string;
  body: string;
  preRequestScript?: string | null;
  postResponseScript?: string | null;
  tests?: string | null;
  docs?: string | null;
}

function fromTab(tab: RequestTab): RequestView {
  const request = tab.request;
  return {
    title: tab.title,
    unsaved: tab.isDirty,
    kind: request.requestType,
    method: request.method,
    url: request.url,
    queryParams: request.queryParams,
    headers: request.headers,
    authType: request.auth.authType,
    bodyMode: request.body.mode,
    body: bodyText(request.body.mode, request.body.content, request.body.formData),
    preRequestScript: request.preRequestScript,
    postResponseScript: request.postResponseScript,
    tests: request.testsScript,
    docs: request.docs,
  };
}

function fromSaved(request: Request): RequestView {
  const mode = request.body?.mode ?? 'none';
  return {
    title: request.name,
    unsaved: false,
    kind: 'http',
    method: request.method,
    url: request.url,
    queryParams: request.queryParams ?? [],
    headers: request.headers,
    authType: request.auth.authType,
    bodyMode: mode,
    body: bodyText(mode, request.body?.content ?? '', request.body?.formData ?? []),
    preRequestScript: request.preRequestScript,
    postResponseScript: request.postResponseScript,
    tests: request.tests,
    docs: request.docs,
  };
}

function renderRequest(view: RequestView, collection: string, path: string): string {
  return [
    `Request: ${view.title}${view.unsaved ? ' (open in the editor, unsaved edits included)' : ''}`,
    `Collection: ${collection}`,
    `Path: ${path}`,
    `Type: ${view.kind}`,
    `${view.method} ${redactKnownSecrets(view.url)}`,
    ...rowsSection('Query parameters', view.queryParams),
    ...rowsSection('Headers', view.headers),
    ...authLine(view.authType),
    ...(view.body ? [`Body (${view.bodyMode}):`, view.body] : []),
    ...codeSection('Pre-request script', view.preRequestScript, 'javascript'),
    ...codeSection('Post-response script', view.postResponseScript, 'javascript'),
    ...codeSection('Tests', view.tests, 'javascript'),
    ...codeSection('Docs', view.docs, 'markdown'),
  ].join('\n');
}

function renderFolder(collection: string, path: string, settings: FolderSettings): string {
  return [
    `Folder: ${path}`,
    `Collection: ${collection}`,
    ...authLine(settings.auth?.authType),
    ...rowsSection('Headers', settings.headers),
    ...variablesSection('Variables', settings.variables),
    ...codeSection('Pre-request script', settings.preRequestScript, 'javascript'),
    ...codeSection('Post-response script', settings.postResponseScript, 'javascript'),
    ...codeSection('Tests', settings.testsScript, 'javascript'),
    ...codeSection('Docs', settings.docs, 'markdown'),
  ].join('\n');
}

function renderCollection(name: string, settings: CollectionSettings): string {
  return [
    `Collection: ${name}`,
    ...authLine(settings.auth?.authType),
    ...rowsSection('Headers', settings.headers),
    ...variablesSection('Variables', settings.variables),
    `The agent may run requests here: ${settings.agentAutonomyEnabled ? 'yes' : 'no'}`,
    ...codeSection('Docs', settings.docs, 'markdown'),
  ].join('\n');
}

function renderEnvironment(collection: string, env: Environment): string {
  return [
    `Environment: ${env.name}`,
    `Collection: ${collection}`,
    ...variablesSection('Variables', env.variables),
  ].join('\n');
}

function renderResponse(tab: RequestTab, response: ResponseState): string {
  const tests = response.testResults ?? [];
  const failed = tests.filter((test) => test.status === 'failed');
  // Response headers are all shown, whatever their enabled flag says.
  const headers = response.headers.map((header) => ({ ...header, enabled: true }));
  return [
    `Last response of: ${tab.title}`,
    `${tab.request.method} ${redactKnownSecrets(tab.request.url)}`,
    `Status: ${response.status} ${response.statusText}`,
    `Time: ${response.durationMs} ms, size: ${response.sizeBytes} bytes`,
    ...rowsSection('Headers', headers),
    ...(tests.length > 0
      ? [
          `Tests: ${tests.length - failed.length} passed, ${failed.length} failed`,
          ...failed.map((test) => `  failed: ${test.name}${test.error ? ` (${test.error})` : ''}`),
        ]
      : []),
    'Body:',
    response.isBinary ? '(binary body, not shared)' : response.body,
  ].join('\n');
}

async function renderChip(chip: ReferenceItem): Promise<string> {
  const path = chip.path ?? '';
  const root = usePaneStore.getState().root;
  switch (chip.kind) {
    case 'request': {
      const tab = findRequestTab(root, chip.collection, path);
      const view = tab ? fromTab(tab) : fromSaved(await getRequest(chip.collection, path));
      return renderRequest(view, chip.collection, path);
    }
    case 'folder':
      return renderFolder(chip.collection, path, await getFolderSettings(chip.collection, path));
    case 'collection':
      return renderCollection(chip.collection, await getCollectionSettings(chip.collection));
    case 'environment':
      return renderEnvironment(chip.collection, await getEnvironment(chip.collection, path));
    case 'last-response': {
      const tab = findRequestTab(root, chip.collection, path);
      if (!tab?.response) return `No response is available for ${chip.label}.`;
      return renderResponse(tab, tab.response);
    }
  }
}

/**
 * Turns a chip into an embedded text resource that Rocket builds from its own data.
 * Secrets are masked, a final `redactKnownSecrets` pass covers free text such as
 * bodies and scripts, and the text is capped at 8 KB. Never rejects: a chip that
 * cannot load becomes a short notice that does not echo the error.
 */
export async function chipToResource(chip: ReferenceItem): Promise<PromptResourceDto> {
  let text: string;
  try {
    text = await renderChip(chip);
  } catch {
    // The error text can echo request content, so it is not passed on.
    text = `Rocket could not load this ${chip.kind}: ${chip.label}.`;
  }
  return { uri: chipUri(chip), mimeType: 'text/plain', text: capText(redactKnownSecrets(text)) };
}
