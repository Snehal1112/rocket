import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { AttachContractInput, CollectionItem } from '@/lib/tauri-api';
import { getCollection } from '@/lib/tauri-api';
import { useContractStore } from '@/stores/contract-store';
import type { ContractTab as ContractTabType } from '@/types/pane-types';

import { ChangelogSummaryBar } from './ChangelogSummaryBar';
import { ChangelogTable } from './ChangelogTable';
import { ContractCard } from './ContractCard';
import { ContractEmptyState } from './ContractEmptyState';
import { ContractForm, type ContractFormValues } from './ContractForm';
import { ContractLivePreview } from './ContractLivePreview';
import { ContractTabTopBar } from './ContractTabTopBar';

// Walk the collection item tree and collect relative folder/request paths.
function collectPaths(
  items: CollectionItem[],
  prefix: string,
  folders: string[],
  requests: string[],
) {
  for (const item of items) {
    if (item.type === 'folder') {
      const seg = item.dirName ?? item.name;
      const path = prefix ? `${prefix}/${seg}` : seg;
      folders.push(path);
      collectPaths(item.items, path, folders, requests);
    } else {
      const seg = item.fileName ?? item.name;
      const path = prefix ? `${prefix}/${seg}` : seg;
      requests.push(path);
    }
  }
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
  project: '',
  version: '',
  effectiveDate: new Date().toISOString().split('T')[0],
  expiryDate: '',
  scopeType: 'collection',
  scopePath: '',
  documentPath: null,
};

interface ContractTabProps {
  tab: ContractTabType;
}

export function ContractTab({ tab }: ContractTabProps) {
  const contractsFor = useContractStore((s) => s.contractsFor);
  const changelogs = useContractStore((s) => s.changelogs);
  const loadContracts = useContractStore((s) => s.loadContracts);
  const attachContract = useContractStore((s) => s.attachContract);
  const removeContract = useContractStore((s) => s.removeContract);
  const loadChangelog = useContractStore((s) => s.loadChangelog);

  const contracts = contractsFor(tab.collectionRoot);

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
      provider: c.provider,
      consumer: c.consumer,
      project: c.project,
      version: c.version,
      effectiveDate: c.effectiveDate,
      expiryDate: c.expiryDate ?? '',
      scopeType,
      scopePath: c.scope.type !== 'collection' ? c.scope.rel_path : '',
      documentPath: c.documentPath,
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
    if (
      !form.title ||
      !form.provider ||
      !form.consumer ||
      !form.project ||
      !form.version ||
      !form.effectiveDate
    ) {
      setError('Title, both teams, project, version, and effective date are required.');
      return;
    }
    if ((form.scopeType === 'folder' || form.scopeType === 'request') && !form.scopePath) {
      setError('Please select a folder or request for the scope.');
      return;
    }

    setSaving(true);
    setError(null);

    try {
      const scope =
        form.scopeType === 'folder'
          ? { type: 'folder' as const, rel_path: form.scopePath }
          : form.scopeType === 'request'
            ? { type: 'request' as const, rel_path: form.scopePath }
            : { type: 'collection' as const };

      const input: AttachContractInput = {
        title: form.title,
        provider: form.provider,
        consumer: form.consumer,
        project: form.project,
        version: form.version,
        effectiveDate: form.effectiveDate,
        expiryDate: form.expiryDate || null,
        documentPath: form.documentPath,
        scope,
        initialSnapshots: [],
      };

      await attachContract(tab.collectionRoot, input);
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
      <div className='flex flex-col h-full bg-background'>
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
      <div className='flex flex-col h-full bg-background'>
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
        <div className='flex items-center justify-end gap-2 px-6 py-3 border-t border-border bg-background shrink-0'>
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
      <div className='flex flex-col h-full bg-background'>
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
                  {contract.provider}
                </span>
                <span className='text-muted-foreground text-xs'>→</span>
                <span className='inline-flex items-center gap-1.5 bg-secondary rounded-full px-2.5 py-1 text-xs'>
                  <span className='w-2 h-2 rounded-full bg-emerald-500 shrink-0' />
                  {contract.consumer}
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
