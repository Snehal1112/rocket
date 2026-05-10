// ─── Enums ────────────────────────────────────────────────

export type ContractStatus =
  | 'active'
  | 'drift'
  | 'breach'
  | 'in_review'
  | 'draft'
  | 'paused'
  | 'expired'
  | 'expiring_in_30_days'
  | 'archived';

export type PartyKind = 'team' | 'company' | 'service' | 'legacy';
export type PartyRole = 'provider' | 'consumer';

/** 'add' | 'remove' | 'modify' — frontend domain names (IPC uses 'added'/'removed'/'changed') */
export type ChangeKind = 'add' | 'remove' | 'modify';

export type ContractScopeType = 'collection' | 'folder' | 'request';

export type ContractScope =
  | { type: 'collection' }
  | { type: 'folder'; rel_path: string }
  | { type: 'request'; rel_path: string };

export type BreakingChangePolicy = 'strict' | 'lenient' | 'additive_ok';

// ─── Entities ─────────────────────────────────────────────

/** A party (provider or consumer) in a contract. */
export interface Party {
  id: string;
  name: string;
  kind: PartyKind;
  /** Seed for avatar color — hash of name if not set */
  avatarSeed?: string;
  /** Hex color override for avatar bg */
  avatarColor?: string;
}

export interface ContractPolicy {
  breakingChangePolicy: BreakingChangePolicy;
  /** Days of notice required before breaking changes land */
  noticeDays: number;
  /** 0–100 percentage. null = no SLA defined */
  uptimeSla: number | null;
}

export interface ChangelogEntry {
  id: string;
  contractId: string;
  /** ISO datetime */
  at: string;
  kind: ChangeKind;
  /** Short human-readable label e.g. "query.limit removed" */
  summary: string;
  /** Full diff detail, optional */
  detail?: string;
  requestId?: string;
  requestMethod?: string;
  requestPath?: string;
  /** True if this change breaks the contract per its policy */
  isBreaking: boolean;
  authorId?: string;
  authorName?: string;
}

/** Map of requestId → shape at time of signing */
export type RequestShapeMap = Record<string, RequestShape>;

export interface RequestShape {
  method: string;
  path: string;
  params: ParamShape[];
  headers: ParamShape[];
  bodySchema?: string; // JSON schema string
}

export interface ParamShape {
  key: string;
  required: boolean;
  type?: string;
}

export interface Contract {
  id: string;
  collectionId: string;
  name: string;
  /** SemVer string e.g. "1.0.2" */
  version: string;
  status: ContractStatus;
  provider: Party;
  consumers: Party[];
  scope: ContractScope;
  policy: ContractPolicy;
  /** ISO date "YYYY-MM-DD" */
  effectiveAt: string;
  /** ISO date or null */
  expiresAt: string | null;
  /**
   * Option B (Tauri): always null on the frontend — snapshot lives in Rust as
   * {id}-snapshot.yml. Kept in the type for forward compat and preview usage.
   */
  signedSnapshot: RequestShapeMap | null;
  /** Cached: derived from signedSnapshot diff in Rust */
  driftCount: number;
  breachCount: number;
  endpointCount: number;
  /** Last 100 changelog entries, returned by Rust IPC */
  changelog: ChangelogEntry[];
  createdBy: string;
  createdAt: string;
  updatedAt: string;

  // Paused-state fields (optional — only present when status === 'paused')
  pausedAt?: string;
  pausedBy?: string;
  pauseReason?: string;
  successorId?: string;
  successorName?: string;
  driftDetectionEnabled?: boolean;

  // Expiry/renewal fields (optional — only present when status === 'expired')
  isArchiveCandidate?: boolean;
  lastRenewalAttemptAt?: string;
  lastRenewalDeclined?: boolean;

  // In-review fields (optional — only present when status === 'in_review')
  reviewerCount?: number;
  commentCount?: number;
}

// ─── Drift ────────────────────────────────────────────────

export interface DriftReport {
  contractId: string;
  computedAt: string;
  diffs: RequestDiff[];
  driftCount: number;
  breachCount: number;
}

export interface RequestDiff {
  requestId: string;
  method: string;
  path: string;
  changes: FieldChange[];
}

export interface FieldChange {
  /** e.g. "params.limit", "method", "body.schema" */
  field: string;
  kind: ChangeKind;
  before?: string;
  after?: string;
  isBreaking: boolean;
}

// ─── Store shape ──────────────────────────────────────────

export interface ContractsState {
  byId: Record<string, Contract>;
  /** collectionId → sorted contractIds (attention-first) */
  byCollection: Record<string, string[]>;
  hoveredId: string | null;
  loading: boolean;
  error: string | null;
}

// ─── Filter / view state ──────────────────────────────────

export type ContractFilterStatus = ContractStatus | 'all';
export type ContractSortKey = 'updated' | 'name' | 'effective' | 'drift';
export type ContractViewMode = 'cards' | 'table';

export interface ContractsFilterState {
  search: string;
  statuses: ContractFilterStatus[];
  sort: ContractSortKey;
  sortDir: 'asc' | 'desc';
  view: ContractViewMode;
}

// ─── Creation form ────────────────────────────────────────

export interface CreateContractFormValues {
  name: string;
  version: string;
  provider: Party;
  consumers: Party[]; // min 1
  scope: ContractScope;
  policy: ContractPolicy;
  effectiveAt: string;
  expiresAt: string | null;
  publishImmediately: boolean;
}

// ─── IPC summaries (returned by Rust commands) ────────────

export interface ContractDriftSummary {
  contractId: string;
  status: ContractStatus;
  driftCount: number;
  breachCount: number;
}

export interface ContractSummary {
  id: string;
  name: string;
  status: ContractStatus;
  driftCount: number;
  breachCount: number;
  endpointCount: number;
}

// ─── Computed helpers ─────────────────────────────────────

export interface ContractCounts {
  total: number;
  active: number;
  drift: number;
  breach: number;
  inReview: number;
  draft: number;
  paused: number;
  expired: number;
  archived: number;
  /** Sum of all driftCount across all contracts (used in "Changes · 30d" card) */
  totalChanges: number;
  /** Breakdown for summary row trend line */
  changesAdded: number;
  changesRemoved: number;
  changesModified: number;
}
