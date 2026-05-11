import * as api from '@/lib/tauri-api';
import { track } from '@/lib/telemetry';
import type {
  ChangeKind,
  ChangelogEntry,
  Contract,
  ContractsState,
  CreateContractFormValues,
} from '@/types/contracts';

type Set = (
  partial:
    | Partial<ContractsState & ContractsActions>
    | ((s: ContractsState & ContractsActions) => Partial<ContractsState & ContractsActions>),
) => void;
type Get = () => ContractsState & ContractsActions;

export interface ContractsActions {
  loadContracts: (collectionId: string) => Promise<void>;
  createContract: (
    collectionId: string,
    values: CreateContractFormValues,
    requests: unknown[],
  ) => Promise<Contract>;
  updateContract: (
    collectionId: string,
    contractId: string,
    values: CreateContractFormValues,
  ) => Promise<Contract>;
  deleteContract: (collectionId: string, id: string) => Promise<void>;
  publishContract: (collectionId: string, id: string) => Promise<void>;
  acceptDrift: (collectionId: string, id: string, newVersion: string) => Promise<void>;
  pauseContract: (collectionId: string, id: string) => Promise<void>;
  resumeContract: (collectionId: string, id: string) => Promise<void>;
  renewContract: (collectionId: string, id: string, newExpiresAt: string | null) => Promise<void>;
  sendForReview: (collectionId: string, id: string) => Promise<void>;
  approveContract: (collectionId: string, id: string) => Promise<void>;
  rejectContract: (collectionId: string, id: string) => Promise<void>;
  archiveContract: (collectionId: string, contractId: string) => Promise<void>;
  unarchiveContract: (collectionId: string, contractId: string) => Promise<void>;
  duplicateContract: (collectionId: string, id: string) => Promise<void>;
  recomputeDrift: (collectionId: string) => Promise<void>;
  loadChangelog: (collectionId: string, contractId: string) => Promise<void>;
}

export function contractsActions(set: Set, get: Get): ContractsActions {
  function upsertInCollection(collectionId: string, contract: Contract) {
    set((state) => {
      const existing = state.byCollection[collectionId] ?? [];
      const ids = existing.includes(contract.id) ? existing : [contract.id, ...existing];
      return {
        byId: { ...state.byId, [contract.id]: contract },
        byCollection: { ...state.byCollection, [collectionId]: ids },
      };
    });
  }

  return {
    loadContracts: async (collectionId) => {
      set({ loading: true, error: null });
      try {
        // collectionId is the collection root path in Tauri
        const raw = await api.listContracts(collectionId);
        // Map IPC Contract → domain Contract (best-effort field mapping)
        const contracts = raw.map(adaptIpcContract);
        const byId: Record<string, Contract> = {};
        const ids: string[] = [];
        for (const c of contracts) {
          byId[c.id] = c;
          ids.push(c.id);
        }
        set((state) => ({
          byId: { ...state.byId, ...byId },
          byCollection: { ...state.byCollection, [collectionId]: ids },
          loading: false,
        }));

        // Load all changelogs in parallel so the Recent Changes panel reflects disk state.
        // Failures are non-fatal — list view still renders without changelog data.
        const loadChangelog = get().loadChangelog;
        await Promise.all(
          ids.map((id) =>
            loadChangelog(collectionId, id).catch((err) =>
              console.warn('[contracts] loadChangelog failed for', id, err),
            ),
          ),
        );
      } catch (err) {
        set({ loading: false, error: String(err) });
      }
    },

    createContract: async (collectionId, values, _requests) => {
      const raw = await api.attachContract(collectionId, {
        title: values.name,
        provider: values.provider,
        consumers: values.consumers,
        version: values.version,
        effectiveDate: values.effectiveAt,
        expiryDate: values.expiresAt,
        documentPaths: [],
        // biome-ignore lint/suspicious/noExplicitAny: bridging domain ContractScope→IPC ContractScope
        scope: values.scope as unknown as any,
        // biome-ignore lint/suspicious/noExplicitAny: bridging domain ContractPolicy→IPC ContractPolicy
        policy: values.policy as unknown as any,
        initialSnapshots: [],
        publishImmediately: values.publishImmediately,
      });
      const contract = adaptIpcContract(raw);
      upsertInCollection(collectionId, contract);
      return contract;
    },

    updateContract: async (collectionId, contractId, values) => {
      const raw = await api.updateContract(collectionId, {
        contractId,
        title: values.name,
        provider: values.provider,
        consumers: values.consumers,
        version: values.version,
        effectiveDate: values.effectiveAt,
        expiryDate: values.expiresAt,
        // biome-ignore lint/suspicious/noExplicitAny: bridging domain ContractPolicy→IPC ContractPolicy
        policy: values.policy as unknown as any,
        newDocumentPaths: [],
        keptDocumentPaths: [],
      });
      const contract = adaptIpcContract(raw);
      // Reload changelog so the Recent Changes panel reflects updated entries.
      const loadChangelog = get().loadChangelog;
      loadChangelog(collectionId, contractId).catch((err) =>
        console.warn('[contracts] loadChangelog failed for', contractId, err),
      );
      upsertInCollection(collectionId, contract);
      return contract;
    },

    deleteContract: async (collectionId, id) => {
      await api.deleteContract(collectionId, id);
      set((state) => {
        const { [id]: _, ...rest } = state.byId;
        return {
          byId: rest,
          byCollection: {
            ...state.byCollection,
            [collectionId]: (state.byCollection[collectionId] ?? []).filter((cid) => cid !== id),
          },
        };
      });
    },

    publishContract: async (collectionId, id) => {
      const prev = get().byId[id]?.status;
      const raw = await api.publishContract(collectionId, id, []);
      const contract = adaptIpcContract(raw);
      upsertInCollection(collectionId, contract);
      try {
        track('contracts.status_changed', { contractId: id, from: prev, to: contract.status });
      } catch (_) {
        /* ignore tracking errors */
      }
    },

    acceptDrift: async (collectionId, id, newVersion) => {
      const prev = get().byId[id]?.status;
      const raw = await api.acceptDrift(collectionId, id, newVersion);
      const contract = adaptIpcContract(raw);
      upsertInCollection(collectionId, contract);
      try {
        track('contracts.status_changed', {
          contractId: id,
          from: prev,
          to: contract.status,
          newVersion,
        });
      } catch (_) {
        /* ignore tracking errors */
      }
    },

    pauseContract: async (collectionId, id) => {
      const prev = get().byId[id]?.status;
      const raw = await api.pauseContract(collectionId, id);
      const contract = adaptIpcContract(raw);
      upsertInCollection(collectionId, contract);
      try {
        track('contracts.status_changed', { contractId: id, from: prev, to: contract.status });
      } catch (_) {
        /* ignore tracking errors */
      }
    },

    resumeContract: async (collectionId, id) => {
      const prev = get().byId[id]?.status;
      const raw = await api.resumeContract(collectionId, id);
      const contract = adaptIpcContract(raw);
      upsertInCollection(collectionId, contract);
      try {
        track('contracts.status_changed', { contractId: id, from: prev, to: contract.status });
      } catch (_) {
        /* ignore tracking errors */
      }
    },

    renewContract: async (collectionId, id, newExpiresAt) => {
      const prev = get().byId[id]?.status;
      const raw = await api.renewContract(collectionId, id, newExpiresAt);
      const contract = adaptIpcContract(raw);
      upsertInCollection(collectionId, contract);
      try {
        track('contracts.status_changed', { contractId: id, from: prev, to: contract.status });
      } catch (_) {
        /* ignore tracking errors */
      }
    },

    sendForReview: async (collectionId, id) => {
      const prev = get().byId[id]?.status;
      const raw = await api.sendForReview(collectionId, id);
      const contract = adaptIpcContract(raw);
      upsertInCollection(collectionId, contract);
      try {
        track('contracts.status_changed', { contractId: id, from: prev, to: contract.status });
      } catch (_) {
        /* ignore tracking errors */
      }
    },

    approveContract: async (collectionId, id) => {
      const prev = get().byId[id]?.status;
      const raw = await api.approveContract(collectionId, id);
      const contract = adaptIpcContract(raw);
      upsertInCollection(collectionId, contract);
      try {
        track('contracts.status_changed', { contractId: id, from: prev, to: contract.status });
      } catch (_) {
        /* ignore tracking errors */
      }
    },

    rejectContract: async (collectionId, id) => {
      const prev = get().byId[id]?.status;
      const raw = await api.rejectContract(collectionId, id);
      const contract = adaptIpcContract(raw);
      upsertInCollection(collectionId, contract);
      try {
        track('contracts.status_changed', { contractId: id, from: prev, to: contract.status });
      } catch (_) {
        /* ignore tracking errors */
      }
    },

    archiveContract: async (collectionId, contractId) => {
      const raw = await api.archiveContract(collectionId, contractId);
      upsertInCollection(collectionId, adaptIpcContract(raw));
    },

    unarchiveContract: async (collectionId, contractId) => {
      const raw = await api.unarchiveContract(collectionId, contractId);
      upsertInCollection(collectionId, adaptIpcContract(raw));
    },

    duplicateContract: async (collectionId, id) => {
      const raw = await api.duplicateContract(collectionId, id);
      upsertInCollection(collectionId, adaptIpcContract(raw));
    },

    recomputeDrift: async (collectionId) => {
      // Snapshot statuses before recompute to detect transitions
      const beforeStatuses: Record<string, string> = {};
      for (const id of get().byCollection[collectionId] ?? []) {
        const status = get().byId[id]?.status;
        if (status) beforeStatuses[id] = status;
      }

      await api.recomputeDrift(collectionId);
      await get().loadContracts(collectionId);

      // Emit drift_detected for every contract that newly entered drift or breach
      for (const id of get().byCollection[collectionId] ?? []) {
        const prev = beforeStatuses[id];
        const curr = get().byId[id];
        if (!curr) continue;
        const nowDrifting = curr.status === 'drift' || curr.status === 'breach';
        const wasDrifting = prev === 'drift' || prev === 'breach';
        if (nowDrifting && !wasDrifting) {
          try {
            track('contracts.drift_detected', {
              contractId: id,
              driftCount: curr.driftCount,
              breachCount: curr.breachCount,
              elapsedMsSinceSigned: curr.createdAt
                ? Date.now() - new Date(curr.createdAt).getTime()
                : undefined,
            });
          } catch (_) {
            /* ignore tracking errors */
          }
        }
      }
    },

    loadChangelog: async (collectionId, contractId) => {
      const ipcLog = await api.getContractChangelog(collectionId, contractId);
      const entries = ipcLog.entries.map((e, i) => adaptIpcChangelogEntry(e, contractId, i));
      set((state) => ({
        byId: {
          ...state.byId,
          [contractId]: state.byId[contractId]
            ? { ...state.byId[contractId], changelog: entries }
            : state.byId[contractId],
        },
      }));
    },
  };
}

/** Maps Rust IPC ChangeType string to domain ChangeKind. */
function adaptChangeKind(changeType: string): ChangeKind {
  if (changeType === 'added') return 'add';
  if (changeType === 'removed') return 'remove';
  return 'modify'; // 'changed'
}

/** Turns a raw field path + change type into a human-readable summary. */
function humanizeSummary(field: string, changeType: string): string {
  const action =
    changeType === 'added' ? 'added' : changeType === 'removed' ? 'removed' : 'changed';
  if (field.startsWith('query_param.')) return `Query parameter ${action}`;
  if (field.startsWith('header.')) return `Header ${action}`;
  if (field.startsWith('form_field.')) return `Form field ${action}`;
  if (field.startsWith('body_field.')) return `Body field ${action}`;
  if (field === 'body') return `Body ${action}`;
  if (field === 'method') return 'HTTP method changed';
  if (field === 'url_pattern') return 'URL pattern changed';
  if (field === 'auth_type') return 'Auth type changed';
  if (field === 'auth_detail') return 'Auth credentials changed';
  // Generic fallback
  const label = field.replace(/_/g, ' ').replace(/\./g, ': ').toLowerCase();
  return `${label.charAt(0).toUpperCase()}${label.slice(1)} ${action}`;
}

/**
 * Adapts an IPC ChangelogEntry to the domain ChangelogEntry.
 * Synthesises fields (id, summary, diffLines) that exist in the domain type
 * but are not present on the Rust wire format.
 */
function adaptIpcChangelogEntry(
  raw: api.ChangelogEntry,
  contractId: string,
  index: number,
): ChangelogEntry {
  // Build structured diff lines from old/new values so the diff block renders.
  const diffLines: import('@/types/contracts').DiffLine[] = [];
  if (raw.oldValue != null && raw.oldValue !== '') {
    diffLines.push({ kind: 'remove', text: raw.oldValue });
  }
  if (raw.newValue != null && raw.newValue !== '') {
    diffLines.push({ kind: 'add', text: raw.newValue });
  }

  const requestPath =
    raw.httpPath ?? (typeof raw.requestPath === 'string' ? raw.requestPath : undefined);

  return {
    id: `${contractId}-${raw.timestamp}-${index}`,
    contractId,
    at: raw.timestamp,
    kind: adaptChangeKind(raw.changeType),
    summary: humanizeSummary(raw.field, raw.changeType),
    detail:
      diffLines.map((l) => `${l.kind === 'remove' ? '−' : '+'} ${l.text}`).join('\n') || undefined,
    diffLines: diffLines.length > 0 ? diffLines : undefined,
    requestPath,
    // Use requestPath as a stable id so the "Open request" action button renders.
    // A real requestId (UUID linking to the request record) would come from the backend.
    requestId: requestPath,
    isBreaking: raw.isBreaking ?? false,
    requestMethod: raw.requestMethod ?? undefined,
    authorName: raw.author ?? undefined,
  };
}

/**
 * Adapts an IPC Contract (from tauri-api.ts) to the domain Contract type.
 * Field names differ between the IPC wire format and the domain model.
 */
function adaptIpcContract(raw: api.Contract): Contract {
  return {
    id: raw.id,
    collectionId: '', // not in IPC response; caller sets via collection root
    name: raw.title,
    version: raw.version ?? '1.0.0',
    status: (raw.status ?? 'active') as Contract['status'],
    provider: {
      id: raw.provider?.id ?? raw.provider?.name?.toLowerCase().replace(/ /g, '-') ?? '',
      name: raw.provider?.name ?? '',
      kind: (raw.provider?.kind ?? 'team') as import('@/types/contracts').PartyKind,
    },
    consumers: (raw.consumers ?? []).map((c) => ({
      id: c.id ?? c.name?.toLowerCase().replace(/ /g, '-') ?? '',
      name: c.name ?? '',
      kind: (c.kind ?? 'team') as import('@/types/contracts').PartyKind,
    })),
    scope: raw.scope as unknown as Contract['scope'],
    policy: {
      breakingChangePolicy:
        (raw.policy?.breakingChangePolicy as import('@/types/contracts').BreakingChangePolicy) ??
        'lenient',
      noticeDays: raw.policy?.noticeDays ?? 30,
      uptimeSla: raw.policy?.uptimeSla ?? null,
    },
    effectiveAt: raw.effectiveDate ?? '',
    expiresAt: raw.expiryDate ?? null,
    signedSnapshot: null, // Option B: always null on frontend
    driftCount: raw.driftCount ?? 0,
    breachCount: raw.breachCount ?? 0,
    endpointCount: raw.endpointCount ?? 0,
    changelog: [], // loaded separately via get_contract_changelog
    createdBy: raw.createdBy ?? '',
    createdAt: raw.createdAt ?? '',
    updatedAt: raw.updatedAt ?? '',
  };
}
