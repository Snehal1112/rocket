import { Lock } from 'lucide-react';
import type { Contract } from '@/lib/tauri-api';
import { ContractCard } from './ContractCard';
import type { ContractFormValues } from './ContractForm';

interface ContractLivePreviewProps {
  values: ContractFormValues;
  collectionRoot: string;
}

export function ContractLivePreview({ values, collectionRoot }: ContractLivePreviewProps) {
  const isEmpty = !values.title && !values.provider && !values.consumer;

  const scope =
    values.scopeType === 'folder'
      ? { type: 'folder' as const, rel_path: values.scopePath || 'select a folder' }
      : values.scopeType === 'request'
        ? { type: 'request' as const, rel_path: values.scopePath || 'select a request' }
        : { type: 'collection' as const };

  const previewContract: Contract = {
    id: 'preview',
    title: values.title || 'Contract title',
    provider: values.provider || 'Provider team',
    consumer: values.consumer || 'Consumer team',
    project: values.project || 'Project name',
    version: values.version || 'v1.0',
    effectiveDate: values.effectiveDate || new Date().toISOString().split('T')[0],
    expiryDate: values.expiryDate || null,
    documentPaths: [...values.existingDocumentPaths, ...values.newDocumentPaths],
    enforcementMode: 'informational',
    scope,
  };

  return (
    <div className='flex flex-col h-full'>
      <p className='text-xs font-medium uppercase tracking-wide text-muted-foreground mb-3'>
        Live preview
      </p>

      {isEmpty ? (
        <div className='flex flex-col items-center justify-center h-44 border border-dashed rounded-lg gap-2 text-muted-foreground'>
          <Lock className='h-5 w-5 opacity-30' />
          <p className='text-xs'>Fill in the form to preview</p>
        </div>
      ) : (
        <ContractCard contract={previewContract} collectionRoot={collectionRoot} preview />
      )}

      {/* Informational hint */}
      <div className='mt-4 border-l-2 border-primary/30 bg-primary/5 rounded-r-md px-3 py-2.5'>
        <p className='text-xs text-muted-foreground leading-relaxed'>
          Once created, RocketAPI snapshots all covered endpoint signatures. Every subsequent save
          is diffed automatically and logged here.
        </p>
      </div>
    </div>
  );
}
