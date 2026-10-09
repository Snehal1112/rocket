import { type AgentProposal, getRequest } from '@/lib/tauri-api';

type Change = AgentProposal['change'];

export interface ProposalTarget {
  collection: string;
  path?: string;
}

export type ProposalPreview =
  | { kind: 'diff' }
  | { kind: 'definition'; text: string }
  | { kind: 'line'; text: string };

export interface ProposalDiff {
  before: string;
  after: string;
  language: string;
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
      return { kind: 'diff' };
    case 'createRequest':
      return { kind: 'definition', text: JSON.stringify(change.request, null, 2) };
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
  if (change.op !== 'updateRequest' && change.op !== 'editScript') return null;
  const language = change.op === 'editScript' ? 'javascript' : 'json';

  const current = await getRequest(change.collection, change.requestPath);
  if (change.op === 'editScript') {
    const before = current[SCRIPT_FIELDS[change.phase]] ?? '';
    return { before, after: change.body, language };
  }

  const patch = asRecord(change.patch);
  const keys = Object.keys(patch).filter((key) => patch[key] !== undefined);
  return {
    before: stringifyPicked(asRecord(current), keys),
    after: stringifyPicked(patch, keys),
    language,
  };
}
