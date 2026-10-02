// src/components/environments/VaultCertificateRow.tsx

import { AlertTriangle, Loader2, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  type ClientCertificate,
  type ExternalSecretBinding,
  listVaultCertificates,
  type VaultCertificateFormat,
  type VaultCertificateSummary,
} from '@/lib/tauri-api';
import {
  isSelectable,
  isWindows,
  needsEcPemWarning,
  vaultCertificateLabel,
} from '@/lib/vault-certificates';

type VaultCertificate = Extract<ClientCertificate, { type: 'vault' }>;

export interface VaultCertificateRowProps {
  idx: number;
  cert: VaultCertificate;
  bindings: ExternalSecretBinding[];
  onChange: (idx: number, patch: Partial<ClientCertificate>) => void;
}

type ListState =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'ready'; certificates: VaultCertificateSummary[] }
  | { status: 'error'; message: string };

interface PickerOption {
  value: string;
  label: string;
  disabled: boolean;
}

const SECTION_LABEL =
  'text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70';

// The body of a `vault` certificate row: which binding, which certificate in its
// vault, and which export format. The list comes from RocketVault and holds names
// and metadata only; the certificate itself is exported when a request needs it.
export function VaultCertificateRow({ idx, cert, bindings, onChange }: VaultCertificateRowProps) {
  const n = idx + 1;
  const format: VaultCertificateFormat = cert.format ?? 'pem';
  const binding = bindings.find((b) => b.alias === cert.binding);
  const connectionId = binding?.connectionId ?? '';
  const vaultName = binding?.vaultName ?? '';
  const windows = useMemo(() => isWindows(), []);
  const [list, setList] = useState<ListState>({ status: 'idle' });
  // Only the latest request may set the list, so a slow answer for a binding the
  // user already left cannot replace the current one.
  const latest = useRef(0);

  const load = useCallback(() => {
    latest.current += 1;
    const request = latest.current;
    if (!connectionId || !vaultName) {
      setList({ status: 'idle' });
      return;
    }
    setList({ status: 'loading' });
    listVaultCertificates(connectionId, vaultName)
      .then((certificates) => {
        if (latest.current === request) setList({ status: 'ready', certificates });
      })
      .catch((err: unknown) => {
        if (latest.current === request) {
          setList({ status: 'error', message: err instanceof Error ? err.message : String(err) });
        }
      });
  }, [connectionId, vaultName]);

  useEffect(() => {
    load();
    return () => {
      latest.current += 1;
    };
  }, [load]);

  const listed = list.status === 'ready' ? list.certificates : [];
  const selected = listed.find((c) => c.name === cert.certificate);
  const ecWarning = needsEcPemWarning(selected?.keyAlgorithm, format, windows);

  const bindingOptions: PickerOption[] = bindings
    .filter((b) => b.alias)
    .map((b) => ({ value: b.alias, label: b.alias, disabled: false }));
  if (cert.binding && !binding) {
    bindingOptions.push({
      value: cert.binding,
      label: `${cert.binding} (not in this environment)`,
      disabled: false,
    });
  }

  const certificateOptions: PickerOption[] = listed.map((c) => ({
    value: c.name,
    label: vaultCertificateLabel(c),
    disabled: !isSelectable(c),
  }));
  // A stored name stays visible. Once the list is loaded and the name is missing, it is marked.
  if (cert.certificate && !selected) {
    certificateOptions.push({
      value: cert.certificate,
      label: list.status === 'ready' ? `${cert.certificate} (not found)` : cert.certificate,
      disabled: false,
    });
  }

  return (
    <div className='space-y-1'>
      <p className={SECTION_LABEL}>RocketVault certificate</p>
      <div className='flex items-center gap-1.5 min-w-0'>
        <Select
          value={cert.binding}
          onValueChange={(v) => {
            if (v !== cert.binding) onChange(idx, { binding: v, certificate: '' });
          }}
        >
          <SelectTrigger
            className='h-7 w-[140px] shrink-0 text-xs font-mono'
            aria-label={`Binding for certificate ${n}`}
          >
            <SelectValue placeholder='Binding' />
          </SelectTrigger>
          <SelectContent>
            {bindingOptions.map((option) => (
              <SelectItem key={option.value} value={option.value} className='text-xs font-mono'>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Select
          value={cert.certificate}
          onValueChange={(v) => onChange(idx, { certificate: v })}
          disabled={!binding}
        >
          <SelectTrigger
            className='h-7 min-w-0 flex-1 text-xs font-mono'
            aria-label={`Vault certificate for certificate ${n}`}
          >
            <SelectValue
              placeholder={
                list.status === 'loading' ? 'Loading certificates' : 'Select a certificate'
              }
            />
          </SelectTrigger>
          <SelectContent>
            {certificateOptions.map((option) => (
              <SelectItem
                key={option.value}
                value={option.value}
                disabled={option.disabled}
                className='text-xs font-mono'
              >
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0'
          onClick={load}
          disabled={!binding || list.status === 'loading'}
          aria-label={`Reload vault certificates for certificate ${n}`}
        >
          {list.status === 'loading' ? (
            <Loader2 className='h-3.5 w-3.5 animate-spin' />
          ) : (
            <RefreshCw className='h-3.5 w-3.5 text-muted-foreground' />
          )}
        </Button>

        <Select
          value={format}
          onValueChange={(v) => onChange(idx, { format: v as VaultCertificateFormat })}
        >
          <SelectTrigger
            className='h-7 w-[96px] shrink-0 text-xs'
            aria-label={`Format for certificate ${n}`}
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value='pem' className='text-xs'>
              PEM
            </SelectItem>
            <SelectItem value='pkcs12' className='text-xs'>
              PKCS12
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      {cert.binding && !binding && (
        <p className='text-[11px] text-destructive'>
          Binding {cert.binding} is not in this environment. Pick one from the list.
        </p>
      )}
      {list.status === 'error' && (
        <p className='text-[11px] text-destructive'>Could not list certificates: {list.message}</p>
      )}
      {list.status === 'ready' && cert.certificate && !selected && (
        <p className='text-[11px] text-destructive'>
          Certificate {cert.certificate} was not found in this vault.
        </p>
      )}
      {ecWarning && (
        <p className='flex items-center gap-1 text-[11px] text-amber-600 dark:text-amber-500'>
          <AlertTriangle className='h-3 w-3 shrink-0' />
          This certificate has an EC key. Windows may not load an EC key from PEM, so choose PKCS12.
        </p>
      )}
    </div>
  );
}
