import * as api from '@/lib/tauri-api';
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
  deleteContract: (collectionId: string, id: string) => Promise<void>;
  publishContract: (collectionId: string, id: string) => Promise<void>;
  pauseContract: (collectionId: string, id: string) => Promise<void>;
  resumeContract: (collectionId: string, id: string) => Promise<void>;
  renewContract: (collectionId: string, id: string, newExpiresAt: string | null) => Promise<void>;
  sendForReview: (collectionId: string, id: string) => Promise<void>;
  approveContract: (collectionId: string, id: string) => Promise<void>;
  rejectContract: (collectionId: string, id: string) => Promise<void>;
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
      const raw = await api.publishContract(collectionId, id, []);
      upsertInCollection(collectionId, adaptIpcContract(raw));
    },

    pauseContract: async (collectionId, id) => {
      const raw = await api.pauseContract(collectionId, id);
      upsertInCollection(collectionId, adaptIpcContract(raw));
    },

    resumeContract: async (collectionId, id) => {
      const raw = await api.resumeContract(collectionId, id);
      upsertInCollection(collectionId, adaptIpcContract(raw));
    },

    renewContract: async (collectionId, id, newExpiresAt) => {
      const raw = await api.renewContract(collectionId, id, newExpiresAt);
      upsertInCollection(collectionId, adaptIpcContract(raw));
    },

    sendForReview: async (collectionId, id) => {
      const raw = await api.sendForReview(collectionId, id);
      upsertInCollection(collectionId, adaptIpcContract(raw));
    },

    approveContract: async (collectionId, id) => {
      const raw = await api.approveContract(collectionId, id);
      upsertInCollection(collectionId, adaptIpcContract(raw));
    },

    rejectContract: async (collectionId, id) => {
      const raw = await api.rejectContract(collectionId, id);
      upsertInCollection(collectionId, adaptIpcContract(raw));
    },

    duplicateContract: async (collectionId, id) => {
      const raw = await api.duplicateContract(collectionId, id);
      upsertInCollection(collectionId, adaptIpcContract(raw));
    },

    recomputeDrift: async (collectionId) => {
      // Calls Rust — returns updated summaries, then re-fetches contracts
      await api.recomputeDrift(collectionId, []);
      // Reload the full contract list to get updated drift counts
      await get().loadContracts(collectionId);
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

/**
 * Adapts an IPC ChangelogEntry to the domain ChangelogEntry.
 * Synthesises fields (id, summary, detail) that exist in the domain type
 * but are not present on the Rust wire format.
 */
function adaptIpcChangelogEntry(
  raw: api.ChangelogEntry,
  contractId: string,
  index: number,
): ChangelogEntry {
  const detail =
    raw.oldValue && raw.newValue
      ? `${raw.oldValue} → ${raw.newValue}`
      : raw.oldValue
        ? `removed: ${raw.oldValue}`
        : raw.newValue
          ? `added: ${raw.newValue}`
          : undefined;

  return {
    id: `${contractId}-${raw.timestamp}-${index}`,
    contractId,
    at: raw.timestamp,
    kind: adaptChangeKind(raw.changeType),
    summary: raw.field,
    detail,
    requestPath: typeof raw.requestPath === 'string' ? raw.requestPath : undefined,
    isBreaking: raw.isBreaking ?? false,
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
