import { useEffect, useRef, useState } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import type {
  AttachContractInput,
  ContractParty,
  UpdateContractInput,
} from '@/lib/tauri-api';
import { getCollection, onCollectionChanged } from '@/lib/tauri-api';
import { collectPaths } from '@/lib/contracts/collectPaths';
import { useContractStore } from '@/stores/contract-store';
import type { ContractTab as ContractTabType } from '@/types/pane-types';

import { ChangelogSummaryBar } from './ChangelogSummaryBar';
import { ChangelogTable } from './ChangelogTable';
import { ContractCard } from './ContractCard';
import { ContractEmptyState } from './ContractEmptyState';
import { ContractForm, type ContractFormValues } from './ContractForm';
import { ContractLivePreview } from './ContractLivePreview';
import { ContractTabTopBar } from './ContractTabTopBar';

// Mirror Rust's ContractParty::from_name: id = name.toLowerCase().replace(/ /g, '-').
function partyFromName(name: string): ContractParty {
  return {
    id: name.toLowerCase().replace(/ /g, '-'),
    name,
    kind: 'team',
  };
}

// View discriminant for internal navigation.
type View =
  | { type: 'list' }
  | { type: 'create' }
  | { type: 'edit'; contractId: string }
  | { type: 'changelog'; contractId: string };

const EMPTY_FORM: ContractFormValues = {
  title: '',
  provider: '',
  consumer: '',
  version: '',
  effectiveDate: new Date().toISOString().split('T')[0],
  expiryDate: '',
  scopeType: 'collection',
  scopePath: '',
  existingDocumentPaths: [],
  newDocumentPaths: [],
};

interface ContractTabProps {
  tab: ContractTabType;
}

export function ContractTab({ tab }: ContractTabProps) {
  const contracts = useContractStore(
    useShallow((s) => s.contractsByRoot[tab.collectionRoot] ?? []),
  );
  const changelogs = useContractStore(useShallow((s) => s.changelogs));
  const loadContracts = useContractStore((s) => s.loadContracts);
  const attachContract = useContractStore((s) => s.attachContract);
  const updateContract = useContractStore((s) => s.updateContract);
  const removeContract = useContractStore((s) => s.removeContract);
  const loadChangelog = useContractStore((s) => s.loadChangelog);

  const [view, setView] = useState<View>({ type: 'list' });
  const [form, setForm] = useState<ContractFormValues>(EMPTY_FORM);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [folders, setFolders] = useState<string[]>([]);
  const [requests, setRequests] = useState<string[]>([]);

  // Load contracts whenever the collection root changes.
  useEffect(() => {
    void loadContracts(tab.collectionRoot);
  }, [tab.collectionRoot, loadContracts]);

  // Pre-load changelogs so ContractCard can show accurate change counts.
  useEffect(() => {
    for (const c of contracts) {
      if (!changelogs[c.id]) {
        void loadChangelog(tab.collectionRoot, c.id);
      }
    }
  }, [contracts, changelogs, tab.collectionRoot, loadChangelog]);

  // Keep refs to the freshest contracts/loadChangelog so the realtime subscription
  // below can read them without resubscribing on every render.
  const contractsRef = useRef(contracts);
  contractsRef.current = contracts;
  const loadChangelogRef = useRef(loadChangelog);
  loadChangelogRef.current = loadChangelog;

  // Refresh changelogs when the watcher reports a change inside this collection.
  // The watcher publishes for any file under <workspace>/collections/, so filter
  // by collection name to avoid reloading on unrelated edits.
  const reloadDebounce = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    let cancelled = false;
    const unlistenPromise = onCollectionChanged((event) => {
      if (cancelled) return;
      // Branch-switch/merge events fire one event per file touched during the
      // checkout/merge — skip them to avoid IPC thrash.
      if (event.type === 'branchSwitched' || event.type === 'branchMerged') return;
      if (event.collection && event.collection !== tab.collectionName) return;
      if (reloadDebounce.current) clearTimeout(reloadDebounce.current);
      reloadDebounce.current = setTimeout(() => {
        for (const c of contractsRef.current) {
          void loadChangelogRef.current(tab.collectionRoot, c.id);
        }
      }, 300);
    });
    return () => {
      cancelled = true;
      if (reloadDebounce.current) clearTimeout(reloadDebounce.current);
      unlistenPromise.then((fn) => fn());
    };
  }, [tab.collectionRoot, tab.collectionName]);

  // Fetch collection tree to populate the scope folder/request dropdowns.
  useEffect(() => {
    getCollection(tab.collectionName)
      .then((col) => {
        const f: string[] = [];
        const r: string[] = [];
        collectPaths(col.root.items, '', f, r);
        setFolders(f);
        setRequests(r);
      })
      .catch(() => {
        // Leave lists empty — dropdowns will show "No folders/requests found".
      });
  }, [tab.collectionName]);

  // ── Navigation helpers ──────────────────────────────────────────
  const goList = () => {
    setView({ type: 'list' });
    setForm(EMPTY_FORM);
    setError(null);
  };

  const goCreate = () => {
    setView({ type: 'create' });
    setForm(EMPTY_FORM);
    setError(null);
  };

  const goEdit = (contractId: string) => {
    const c = contracts.find((x) => x.id === contractId);
    if (!c) return;
    const scopeType =
      c.scope.type === 'folder' ? 'folder' : c.scope.type === 'request' ? 'request' : 'collection';
    setForm({
      title: c.title,
      provider: c.provider.name,
      consumer: c.consumers[0]?.name ?? '',
      version: c.version,
      effectiveDate: c.effectiveDate,
      expiryDate: c.expiryDate ?? '',
      scopeType,
      scopePath: c.scope.type !== 'collection' ? c.scope.rel_path : '',
      existingDocumentPaths: c.documentPaths,
      newDocumentPaths: [],
    });
    setView({ type: 'edit', contractId });
    setError(null);
  };

  const goChangelog = async (contractId: string) => {
    await loadChangelog(tab.collectionRoot, contractId);
    setView({ type: 'changelog', contractId });
  };

  // ── Submit ─────────────────────────────────────────────────────
  const handleSubmit = async () => {
    if (!form.title || !form.provider || !form.consumer || !form.version || !form.effectiveDate) {
      setError('Title, both teams, version, and effective date are required.');
      return;
    }
    if ((form.scopeType === 'folder' || form.scopeType === 'request') && !form.scopePath) {
      setError('Please select a folder or request for the scope.');
      return;
    }

    setSaving(true);
    setError(null);

    try {
      if (view.type === 'edit') {
        const input: UpdateContractInput = {
          contractId: view.contractId,
          title: form.title,
          provider: partyFromName(form.provider),
          consumers: form.consumer ? [partyFromName(form.consumer)] : [],
          version: form.version,
          effectiveDate: form.effectiveDate,
          expiryDate: form.expiryDate || null,
          policy: { breakingChangePolicy: 'strict', noticeDays: 30 },
          newDocumentPaths: form.newDocumentPaths,
          keptDocumentPaths: form.existingDocumentPaths,
        };
        await updateContract(tab.collectionRoot, input);
      } else {
        const scope =
          form.scopeType === 'folder'
            ? { type: 'folder' as const, rel_path: form.scopePath }
            : form.scopeType === 'request'
              ? { type: 'request' as const, rel_path: form.scopePath }
              : { type: 'collection' as const };

        const input: AttachContractInput = {
          title: form.title,
          provider: partyFromName(form.provider),
          consumers: form.consumer ? [partyFromName(form.consumer)] : [],
          version: form.version,
          effectiveDate: form.effectiveDate,
          expiryDate: form.expiryDate || null,
          documentPaths: form.newDocumentPaths,
          scope,
          policy: { breakingChangePolicy: 'strict', noticeDays: 30 },
          initialSnapshots: [],
          publishImmediately: false,
        };
        await attachContract(tab.collectionRoot, input);
      }
      goList();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  // ── Delete ─────────────────────────────────────────────────────
  const handleDelete = async (contractId: string) => {
    await removeContract(tab.collectionRoot, contractId);
  };

  // ── Render: List view ──────────────────────────────────────────
  if (view.type === 'list') {
    return (
      <div className='flex flex-col h-full bg-card'>
        <ContractTabTopBar collectionName={tab.collectionName} view='list' onNew={goCreate} />
        <ScrollArea className='flex-1'>
          <div className='max-w-2xl mx-auto px-6 py-5'>
            {contracts.length === 0 ? (
              <ContractEmptyState onNew={goCreate} />
            ) : (
              <div className='space-y-3'>
                {contracts.map((c) => (
                  <ContractCard
                    key={c.id}
                    contract={c}
                    collectionRoot={tab.collectionRoot}
                    onViewChangelog={() => void goChangelog(c.id)}
                    onEdit={() => goEdit(c.id)}
                    onDelete={() => void handleDelete(c.id)}
                  />
                ))}
              </div>
            )}
          </div>
        </ScrollArea>
      </div>
    );
  }

  // ── Render: Create / Edit view ─────────────────────────────────
  if (view.type === 'create' || view.type === 'edit') {
    const isEdit = view.type === 'edit';
    const viewTitle = isEdit ? 'Edit contract' : 'New contract';

    return (
      <div className='flex flex-col h-full bg-card'>
        <ContractTabTopBar
          collectionName={tab.collectionName}
          view={isEdit ? 'edit' : 'create'}
          viewTitle={viewTitle}
          onBack={goList}
        />

        <div className='flex flex-1 overflow-hidden'>
          {/* Left: form */}
          <div className='w-[380px] shrink-0 border-r border-border px-6 py-5 overflow-y-auto'>
            <ContractForm
              values={form}
              onChange={setForm}
              folders={folders}
              requests={requests}
              error={error}
            />
          </div>

          {/* Right: live preview */}
          <div className='flex-1 px-6 py-5 bg-muted/30 overflow-y-auto'>
            <ContractLivePreview values={form} collectionRoot={tab.collectionRoot} />
          </div>
        </div>

        {/* Action bar */}
        <div className='flex items-center justify-end gap-2 px-6 py-3 border-t border-border bg-card shrink-0'>
          <Button variant='outline' size='sm' onClick={goList}>
            Cancel
          </Button>
          <Button size='sm' onClick={() => void handleSubmit()} disabled={saving}>
            {saving
              ? isEdit
                ? 'Saving…'
                : 'Creating…'
              : isEdit
                ? 'Save changes'
                : 'Create contract'}
          </Button>
        </div>
      </div>
    );
  }

  // ── Render: Changelog view ─────────────────────────────────────
  if (view.type === 'changelog') {
    const contract = contracts.find((c) => c.id === view.contractId);
    const changelog = changelogs[view.contractId];

    return (
      <div className='flex flex-col h-full bg-card'>
        <ContractTabTopBar
          collectionName={tab.collectionName}
          view='changelog'
          viewTitle={contract ? `${contract.title} — Changelog` : 'Changelog'}
          onBack={goList}
        />
        <ScrollArea className='flex-1'>
          <div className='max-w-3xl mx-auto px-6 py-5'>
            {/* Parties summary */}
            {contract && (
              <div className='flex items-center gap-2 flex-wrap mb-5'>
                <span className='inline-flex items-center gap-1.5 bg-secondary rounded-full px-2.5 py-1 text-xs'>
                  <span className='w-2 h-2 rounded-full bg-violet-500 shrink-0' />
                  {contract.provider.name}
                </span>
                <span className='text-muted-foreground text-xs'>→</span>
                <span className='inline-flex items-center gap-1.5 bg-secondary rounded-full px-2.5 py-1 text-xs'>
                  <span className='w-2 h-2 rounded-full bg-emerald-500 shrink-0' />
                  {contract.consumers[0]?.name ?? '—'}
                </span>
              </div>
            )}

            {/* Metric cards + table */}
            {changelog ? (
              <>
                <ChangelogSummaryBar changelog={changelog} />
                <ChangelogTable entries={changelog.entries} />
              </>
            ) : (
              <p className='text-sm text-muted-foreground'>Loading changelog…</p>
            )}
          </div>
        </ScrollArea>
      </div>
    );
  }

  return null;
}
