import type {
  BreakingChangePolicy,
  Contract,
  DriftReport,
  FieldChange,
  RequestDiff,
} from '@/types/contracts';
import { type CollectionRequest, computeSnapshot } from './snapshot';

/**
 * PREVIEW-ONLY frontend drift engine.
 *
 * Option B: This function is used exclusively in NewContractModal to show a
 * live "what would change" preview. It is NOT used to update the Zustand store.
 * The authoritative drift computation happens in Rust via recompute_drift.
 *
 * Returns empty report for draft/paused contracts or when signedSnapshot is null.
 */
export function computeDrift(
  contract: Contract,
  currentRequests: CollectionRequest[],
): DriftReport {
  const empty: DriftReport = {
    contractId: contract.id,
    computedAt: new Date().toISOString(),
    diffs: [],
    driftCount: 0,
    breachCount: 0,
  };

  // Paused contracts are not monitored
  if (contract.status === 'paused') return empty;

  // Draft / unpublished contracts have no snapshot yet
  if (!contract.signedSnapshot) return empty;

  const currentSnapshot = computeSnapshot(currentRequests, contract.scope);
  const diffs: RequestDiff[] = [];
  const policy = contract.policy.breakingChangePolicy;

  // Diff each snapshotted request against current
  for (const [reqId, signed] of Object.entries(contract.signedSnapshot)) {
    const current = currentSnapshot[reqId];
    const changes: FieldChange[] = [];

    if (!current) {
      // Entire request removed — always breaking
      changes.push({
        field: 'request',
        kind: 'remove',
        before: `${signed.method} ${signed.path}`,
        isBreaking: true,
      });
    } else {
      if (current.method !== signed.method) {
        changes.push({
          field: 'method',
          kind: 'modify',
          before: signed.method,
          after: current.method,
          isBreaking: true,
        });
      }
      if (current.path !== signed.path) {
        changes.push({
          field: 'path',
          kind: 'modify',
          before: signed.path,
          after: current.path,
          isBreaking: true,
        });
      }
      changes.push(...diffParams('params', signed.params, current.params, policy));
      changes.push(...diffParams('headers', signed.headers, current.headers, policy));
    }

    if (changes.length > 0) {
      diffs.push({ requestId: reqId, method: signed.method, path: signed.path, changes });
    }
  }

  // New endpoints added to collection
  for (const [reqId, current] of Object.entries(currentSnapshot)) {
    if (!contract.signedSnapshot?.[reqId]) {
      diffs.push({
        requestId: reqId,
        method: current.method,
        path: current.path,
        changes: [
          {
            field: 'request',
            kind: 'add',
            after: `${current.method} ${current.path}`,
            isBreaking: policy === 'strict',
          },
        ],
      });
    }
  }

  const driftCount = diffs.reduce((n, d) => n + d.changes.length, 0);
  const breachCount = diffs.reduce((n, d) => n + d.changes.filter((c) => c.isBreaking).length, 0);

  return {
    contractId: contract.id,
    computedAt: new Date().toISOString(),
    diffs,
    driftCount,
    breachCount,
  };
}

function diffParams(
  prefix: string,
  signed: Array<{ key: string; required: boolean }>,
  current: Array<{ key: string; required: boolean }>,
  policy: BreakingChangePolicy,
): FieldChange[] {
  const changes: FieldChange[] = [];
  const signedMap = Object.fromEntries(signed.map((p) => [p.key, p]));
  const currentMap = Object.fromEntries(current.map((p) => [p.key, p]));

  // Removed params
  for (const [key, sp] of Object.entries(signedMap)) {
    if (!currentMap[key]) {
      // Required param removed → always breaking; optional → breaking except additive_ok
      const isBreaking = sp.required || policy !== 'additive_ok';
      changes.push({ field: `${prefix}.${key}`, kind: 'remove', before: key, isBreaking });
    } else if (currentMap[key].required !== sp.required) {
      // optional → required is breaking
      const isBreaking = !sp.required && currentMap[key].required;
      changes.push({
        field: `${prefix}.${key}.required`,
        kind: 'modify',
        before: String(sp.required),
        after: String(currentMap[key].required),
        isBreaking,
      });
    }
  }

  // Added params
  for (const key of Object.keys(currentMap)) {
    if (!signedMap[key]) {
      changes.push({
        field: `${prefix}.${key}`,
        kind: 'add',
        after: key,
        isBreaking: policy === 'strict',
      });
    }
  }

  return changes;
}
