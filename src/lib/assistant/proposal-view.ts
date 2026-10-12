import { type AgentProposal, getFolderSettings, getRequest } from '@/lib/tauri-api';

type Change = AgentProposal['change'];

export interface ProposalTarget {
  collection: string;
  path?: string;
}

export type ProposalPreview =
  | { kind: 'diff' }
  | { kind: 'definition'; text: string; docs?: string }
  | { kind: 'line'; text: string };

export interface ProposalDiff {
  before: string;
  after: string;
  language: string;
  /** The docs change, shown as Markdown apart from the JSON. Present only when docs change. */
  docs?: { before: string; after: string };
}

type ScriptPhase = Extract<Change, { op: 'editScript' }>['phase'];

// Where each proposal phase is stored on a saved request.
const SCRIPT_FIELDS: Record<ScriptPhase, 'preRequestScript' | 'postResponseScript' | 'tests'> = {
  preRequest: 'preRequestScript',
  postResponse: 'postResponseScript',
  tests: 'tests',
};

function joinPath(parent: string, name: string): string {
  return parent ? `${parent}/${name}` : name;
}

// Lets a typed DTO be read key by key.
function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {};
}

function stringifyPicked(source: Record<string, unknown>, keys: string[]): string {
  const picked: Record<string, unknown> = {};
  for (const key of keys) picked[key] = source[key] ?? null;
  return JSON.stringify(picked, null, 2);
}

// The text of a docs value. A missing or non-text value counts as empty.
function docsText(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

// The keys of a patch that go into the JSON diff, and the docs change when there is one.
function splitDocs(
  patch: Record<string, unknown>,
  currentDocs: unknown,
): { keys: string[]; docs?: { before: string; after: string } } {
  const keys = Object.keys(patch).filter((key) => patch[key] !== undefined && key !== 'docs');
  if (typeof patch.docs !== 'string') return { keys };
  return { keys, docs: { before: docsText(currentDocs), after: patch.docs } };
}

/** Where the change lands, for the card header and the open-tab checks. */
export function proposalTarget(change: Change): ProposalTarget {
  switch (change.op) {
    case 'createFolder':
      return { collection: change.collection, path: joinPath(change.parentPath, change.name) };
    case 'createRequest':
      return { collection: change.collection, path: change.folderPath || undefined };
    case 'updateRequest':
    case 'editScript':
      return { collection: change.collection, path: change.requestPath };
    case 'updateFolderSettings':
      return { collection: change.collection, path: change.folderPath };
    case 'moveItem':
      return { collection: change.collection, path: change.fromPath };
    case 'renameItem':
      return { collection: change.collection, path: change.path };
    case 'setEnvVar':
      return { collection: change.collection };
  }
}

/** How the card previews the change. */
export function proposalPreview(change: Change): ProposalPreview {
  switch (change.op) {
    case 'updateRequest':
    case 'editScript':
    case 'updateFolderSettings':
      return { kind: 'diff' };
    case 'createRequest': {
      const { docs, ...rest } = change.request;
      return {
        kind: 'definition',
        text: JSON.stringify(rest, null, 2),
        ...(typeof docs === 'string' && docs !== '' ? { docs } : {}),
      };
    }
    case 'createFolder':
      return { kind: 'line', text: `New folder ${joinPath(change.parentPath, change.name)}` };
    case 'moveItem':
      return {
        kind: 'line',
        text: `Move ${change.fromPath} to ${change.toFolder || 'the collection root'}`,
      };
    case 'renameItem':
      return { kind: 'line', text: `Rename ${change.path} to ${change.newName}` };
    case 'setEnvVar':
      return {
        kind: 'line',
        text: `Set ${change.key} = ${change.value} in environment ${change.environment}`,
      };
  }
}

/** Why a failed proposal could not apply, when the backend said. */
export function proposalFailure(proposal: AgentProposal): string | undefined {
  return proposal.statusMessage || undefined;
}

/**
 * Before and after texts for a script edit or request update. The proposal
 * DTO carries only the new values, so the before side is the stored request.
 */
export async function loadProposalDiff(proposal: AgentProposal): Promise<ProposalDiff | null> {
  const { change } = proposal;
  if (change.op === 'updateFolderSettings') {
    const current = await getFolderSettings(change.collection, change.folderPath);
    const patch = asRecord(change.patch);
    const { keys, docs } = splitDocs(patch, current.docs);
    // Variables are shown as key, value and enabled only, like the proposal.
    const before = { ...asRecord(current) };
    before.variables = current.variables.map(({ key, value, enabled }) => ({
      key,
      value,
      enabled,
    }));
    return {
      before: stringifyPicked(before, keys),
      after: stringifyPicked(patch, keys),
      language: 'json',
      ...(docs ? { docs } : {}),
    };
  }
  if (change.op !== 'updateRequest' && change.op !== 'editScript') return null;
  const language = change.op === 'editScript' ? 'javascript' : 'json';

  const current = await getRequest(change.collection, change.requestPath);
  if (change.op === 'editScript') {
    const before = current[SCRIPT_FIELDS[change.phase]] ?? '';
    return { before, after: change.body, language };
  }

  const patch = asRecord(change.patch);
  const { keys, docs } = splitDocs(patch, current.docs);
  return {
    before: stringifyPicked(asRecord(current), keys),
    after: stringifyPicked(patch, keys),
    language,
    ...(docs ? { docs } : {}),
  };
}
