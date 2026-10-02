// src/components/environments/CertificatesTab.tsx

import { open } from '@tauri-apps/plugin-dialog';
import { ArrowDown, ArrowUp, Check, FolderOpen, Loader2, Plus, Save, X } from 'lucide-react';
import { useMemo } from 'react';
import { toast } from 'sonner';
import { SingleLineEditor } from '@/components/editor';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { SaveButtonState } from '@/hooks/use-save-button';
import { isLiteralPassphrase } from '@/lib/certificate-validation';
import type { ClientCertificate, ExternalSecretBinding } from '@/lib/tauri-api';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import { type VaultSecretOption, vaultSecretOptions } from '@/lib/vault-secret-options';

type FileCertificate = Exclude<ClientCertificate, { type: 'vault' }>;

const TYPE_LABELS: Record<ClientCertificate['type'], string> = {
  pem: 'PEM',
  pkcs12: 'PKCS12',
  vault: 'Vault',
};

export interface CertificatesTabProps {
  certificates: ClientCertificate[];
  bindings: ExternalSecretBinding[];
  onChange: (idx: number, patch: Partial<ClientCertificate>) => void;
  onAdd: (type: 'pem' | 'pkcs12') => void;
  onRemove: (idx: number) => void;
  onMove: (idx: number, direction: -1 | 1) => void;
  onSave: () => void;
  isDirty: boolean;
  saveState: SaveButtonState;
  variableContext?: Map<string, VariableScopeEntry>;
}

type PieceSource = 'file' | 'vault';

// One piece of material (certificate, private key or PKCS12 bundle) and the
// patches that edit it. A piece is a vault piece when its secret is a string.
interface PieceSpec {
  label: string;
  filePath: string;
  secret: string | undefined;
  withFilePath: (path: string) => Partial<ClientCertificate>;
  withSecret: (secret: string) => Partial<ClientCertificate>;
  toFile: Partial<ClientCertificate>;
  toVault: Partial<ClientCertificate>;
}

function pieceSpecs(cert: FileCertificate): PieceSpec[] {
  if (cert.type === 'pem') {
    return [
      {
        label: 'Certificate',
        filePath: cert.certificateFilePath ?? '',
        secret: cert.certificateSecret,
        withFilePath: (path) => ({ certificateFilePath: path }),
        withSecret: (secret) => ({ certificateSecret: secret }),
        toFile: { certificateSecret: undefined },
        toVault: { certificateFilePath: '', certificateSecret: '' },
      },
      {
        label: 'Private key',
        filePath: cert.privateKeyFilePath ?? '',
        secret: cert.privateKeySecret,
        withFilePath: (path) => ({ privateKeyFilePath: path }),
        withSecret: (secret) => ({ privateKeySecret: secret }),
        toFile: { privateKeySecret: undefined },
        toVault: { privateKeyFilePath: '', privateKeySecret: '' },
      },
    ];
  }
  return [
    {
      label: 'PKCS12 bundle',
      filePath: cert.pkcs12FilePath ?? '',
      secret: cert.pkcs12Secret,
      withFilePath: (path) => ({ pkcs12FilePath: path }),
      withSecret: (secret) => ({ pkcs12Secret: secret }),
      toFile: { pkcs12Secret: undefined },
      toVault: { pkcs12FilePath: '', pkcs12Secret: '' },
    },
  ];
}

export function CertificatesTab({
  certificates,
  bindings,
  onChange,
  onAdd,
  onRemove,
  onMove,
  onSave,
  isDirty,
  saveState,
  variableContext,
}: CertificatesTabProps) {
  const options = useMemo(() => vaultSecretOptions(bindings), [bindings]);

  return (
    <div className='flex-1 flex flex-col min-w-0'>
      <div className='px-3 pt-3 pb-2 border-b border-border/40 shrink-0 space-y-0.5'>
        <p className='text-[11px] text-muted-foreground'>
          Domain: use * as a wildcard, for example *.example.com, and add :port to match one port.
          The first matching certificate is used, so put specific domains before wildcards.
        </p>
        <p className='text-[11px] text-muted-foreground'>
          Relative paths start at the collection folder. Encrypted PEM keys need their passphrase.
        </p>
      </div>

      {certificates.length === 0 ? (
        <div className='flex-1 flex flex-col items-center justify-center gap-1 text-center px-6'>
          <p className='text-sm font-medium text-foreground'>No client certificates</p>
          <p className='text-xs text-muted-foreground leading-relaxed max-w-[280px]'>
            Add a PEM or PKCS12 certificate to present it to matching hosts.
          </p>
        </div>
      ) : (
        <ScrollArea className='flex-1'>
          <div className='px-3 pt-2 pb-1 space-y-3'>
            {certificates.map((cert, idx) => (
              <CertificateRow
                // biome-ignore lint/suspicious/noArrayIndexKey: rows are fully controlled and hold no local state
                key={idx}
                idx={idx}
                total={certificates.length}
                cert={cert}
                options={options}
                onChange={onChange}
                onRemove={onRemove}
                onMove={onMove}
                variableContext={variableContext}
              />
            ))}
          </div>
        </ScrollArea>
      )}

      <div className='px-3 py-2 border-t border-border/40 flex items-center justify-between shrink-0'>
        <div className='flex items-center gap-1'>
          <Button
            variant='ghost'
            size='sm'
            onClick={() => onAdd('pem')}
            className='h-7 text-xs text-muted-foreground hover:text-foreground gap-1.5'
          >
            <Plus className='h-3.5 w-3.5' />
            Add PEM
          </Button>
          <Button
            variant='ghost'
            size='sm'
            onClick={() => onAdd('pkcs12')}
            className='h-7 text-xs text-muted-foreground hover:text-foreground gap-1.5'
          >
            <Plus className='h-3.5 w-3.5' />
            Add PKCS12
          </Button>
        </div>
        <Button
          size='sm'
          onClick={onSave}
          disabled={!isDirty || saveState !== 'idle'}
          className={cn('gap-1.5', saveState === 'success' && 'text-green-600')}
        >
          {saveState === 'saving' ? (
            <Loader2 className='h-3.5 w-3.5 animate-spin' />
          ) : saveState === 'success' ? (
            <Check className='h-3.5 w-3.5' />
          ) : (
            <Save className='h-3.5 w-3.5' />
          )}
          {saveState === 'success' ? 'Saved' : 'Save'}
        </Button>
      </div>
    </div>
  );
}

interface CertificateRowProps {
  idx: number;
  total: number;
  cert: ClientCertificate;
  options: VaultSecretOption[];
  onChange: (idx: number, patch: Partial<ClientCertificate>) => void;
  onRemove: (idx: number) => void;
  onMove: (idx: number, direction: -1 | 1) => void;
  variableContext?: Map<string, VariableScopeEntry>;
}

function CertificateRow({
  idx,
  total,
  cert,
  options,
  onChange,
  onRemove,
  onMove,
  variableContext,
}: CertificateRowProps) {
  const n = idx + 1;

  return (
    <div className='space-y-2 pb-3 border-b border-border/20 last:border-0'>
      <div className='flex items-center gap-1.5 min-w-0'>
        <Badge variant='secondary' className='text-[11px] shrink-0'>
          {TYPE_LABELS[cert.type]}
        </Badge>
        <div className='flex-1 min-w-0'>
          <SingleLineEditor
            aria-label={`Domain for certificate ${n}`}
            placeholder='Domain, for example *.example.com'
            value={cert.domain}
            onChange={(domain) => onChange(idx, { domain })}
            variableContext={variableContext}
            className='h-7 text-xs font-mono'
          />
        </div>
        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0'
          disabled={idx === 0}
          onClick={() => onMove(idx, -1)}
          aria-label={`Move certificate ${n} up`}
        >
          <ArrowUp className='h-3.5 w-3.5 text-muted-foreground' />
        </Button>
        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0'
          disabled={idx === total - 1}
          onClick={() => onMove(idx, 1)}
          aria-label={`Move certificate ${n} down`}
        >
          <ArrowDown className='h-3.5 w-3.5 text-muted-foreground' />
        </Button>
        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0'
          onClick={() => onRemove(idx)}
          aria-label={`Delete certificate ${n}`}
        >
          <X className='h-3.5 w-3.5 text-muted-foreground hover:text-destructive' />
        </Button>
      </div>

      {cert.type !== 'vault' && (
        <>
          {pieceSpecs(cert).map((spec) => (
            <PieceField
              key={spec.label}
              certNumber={n}
              idx={idx}
              spec={spec}
              options={options}
              onChange={onChange}
              variableContext={variableContext}
            />
          ))}

          <PassphraseField
            idx={idx}
            certNumber={n}
            passphrase={cert.passphrase ?? ''}
            options={options}
            onChange={onChange}
            variableContext={variableContext}
          />
        </>
      )}
    </div>
  );
}

interface PieceFieldProps {
  idx: number;
  certNumber: number;
  spec: PieceSpec;
  options: VaultSecretOption[];
  onChange: (idx: number, patch: Partial<ClientCertificate>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
}

function PieceField({
  idx,
  certNumber,
  spec,
  options,
  onChange,
  variableContext,
}: PieceFieldProps) {
  const source: PieceSource = spec.secret !== undefined ? 'vault' : 'file';
  const name = `${spec.label} `;
  const lower = spec.label.toLowerCase();

  // A saved reference can point at a secret that is no longer fetched. Keep it visible.
  const secretOptions =
    spec.secret && !options.some((o) => o.value === spec.secret)
      ? [...options, { value: spec.secret, label: `${spec.secret} (not fetched)` }]
      : options;

  const browse = async () => {
    try {
      const picked = await open({ multiple: false, title: `Select ${lower} file` });
      if (typeof picked === 'string' && picked) onChange(idx, spec.withFilePath(picked));
    } catch (err) {
      console.error('[CertificatesTab] file picker failed:', err);
      toast.error('Could not open the file picker');
    }
  };

  return (
    <div className='space-y-1'>
      <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
        {spec.label}
      </p>
      <div className='flex items-center gap-1.5 min-w-0'>
        <Select
          value={source}
          onValueChange={(v) => {
            if (v === source) return;
            onChange(idx, v === 'vault' ? spec.toVault : spec.toFile);
          }}
        >
          <SelectTrigger
            className='h-7 w-[120px] shrink-0 text-xs'
            aria-label={`${name}source for certificate ${certNumber}`}
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value='file' className='text-xs'>
              File
            </SelectItem>
            <SelectItem value='vault' className='text-xs'>
              Vault secret
            </SelectItem>
          </SelectContent>
        </Select>

        {source === 'file' ? (
          <>
            <div className='flex-1 min-w-0'>
              <SingleLineEditor
                aria-label={`${name}file path for certificate ${certNumber}`}
                placeholder='Path to the file'
                value={spec.filePath}
                onChange={(path) => onChange(idx, spec.withFilePath(path))}
                variableContext={variableContext}
                className='h-7 text-xs font-mono'
              />
            </div>
            <Button
              variant='outline'
              size='sm'
              className='h-7 text-xs gap-1.5 shrink-0'
              onClick={() => void browse()}
              aria-label={`Browse for ${lower} file for certificate ${certNumber}`}
            >
              <FolderOpen className='h-3.5 w-3.5' />
              Browse
            </Button>
          </>
        ) : (
          <Select
            value={spec.secret ?? ''}
            onValueChange={(v) => onChange(idx, spec.withSecret(v))}
          >
            <SelectTrigger
              className='h-7 min-w-0 flex-1 text-xs font-mono'
              aria-label={`${name}vault secret for certificate ${certNumber}`}
            >
              <SelectValue placeholder='Select a vault secret' />
            </SelectTrigger>
            <SelectContent>
              {secretOptions.map((option) => (
                <SelectItem key={option.value} value={option.value} className='text-xs font-mono'>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </div>
    </div>
  );
}

interface PassphraseFieldProps {
  idx: number;
  certNumber: number;
  passphrase: string;
  options: VaultSecretOption[];
  onChange: (idx: number, patch: Partial<ClientCertificate>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
}

function PassphraseField({
  idx,
  certNumber,
  passphrase,
  options,
  onChange,
  variableContext,
}: PassphraseFieldProps) {
  return (
    <div className='space-y-1'>
      <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
        Passphrase
      </p>
      <div className='flex items-center gap-1.5 min-w-0'>
        <div className='flex-1 min-w-0'>
          <SingleLineEditor
            aria-label={`Passphrase for certificate ${certNumber}`}
            placeholder='Optional, for example {{vault.NAME}}'
            value={passphrase}
            onChange={(value) => onChange(idx, { passphrase: value === '' ? undefined : value })}
            isSecret
            variableContext={variableContext}
            className='h-7 text-xs font-mono'
          />
        </div>
        <Select
          value=''
          onValueChange={(v) => onChange(idx, { passphrase: `{{${v}}}` })}
          disabled={options.length === 0}
        >
          <SelectTrigger
            className='h-7 w-[170px] shrink-0 text-xs'
            aria-label={`Insert vault secret into passphrase for certificate ${certNumber}`}
          >
            <SelectValue placeholder='Insert vault secret' />
          </SelectTrigger>
          <SelectContent>
            {options.map((option) => (
              <SelectItem key={option.value} value={option.value} className='text-xs font-mono'>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      {isLiteralPassphrase(passphrase) && (
        <p className='text-[11px] text-amber-600 dark:text-amber-500'>
          This passphrase is saved in the environment file as typed. Use a vault secret placeholder
          such as {'{{vault.NAME}}'} instead.
        </p>
      )}
    </div>
  );
}
