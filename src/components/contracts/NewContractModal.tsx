import { useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { useContractsStore } from '@/stores/contracts/contractsSlice';
import type { ContractPolicy, ContractScope, Party } from '@/types/contracts';

interface NewContractModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  collectionId: string;
  collectionName: string;
}

interface FormState {
  name: string;
  version: string;
  providerName: string;
  /** Comma-separated for MVP — SSO combobox in future sprint */
  consumerNames: string;
  scopeType: 'collection' | 'folder' | 'request';
  scopePath: string;
  effectiveAt: string;
  expiresAt: string;
  breakingChangePolicy: 'strict' | 'lenient' | 'additive_ok';
  noticeDays: string;
  uptimeSla: string;
}

type FormErrors = Partial<Record<keyof FormState | '_global', string>>;

function todayISO(): string {
  return new Date().toISOString().split('T')[0];
}

function slugify(s: string): string {
  return s
    .toLowerCase()
    .trim()
    .replace(/\s+/g, '-')
    .replace(/[^a-z0-9-]/g, '');
}

function buildParties(csv: string): Party[] {
  return csv
    .split(',')
    .map((n) => n.trim())
    .filter(Boolean)
    .map((name) => ({ id: slugify(name), name, kind: 'team' as const }));
}

function validate(f: FormState): FormErrors {
  const e: FormErrors = {};
  if (f.name.trim().length < 2) e.name = 'At least 2 characters';
  if (!/^\d+\.\d+\.\d+/.test(f.version.trim())) e.version = 'Must be semver, e.g. 1.0.0';
  if (!f.providerName.trim()) e.providerName = 'Required';
  if (!f.consumerNames.trim()) e.consumerNames = 'At least one consumer required';
  if (!f.effectiveAt) e.effectiveAt = 'Required';
  if (f.expiresAt && f.expiresAt <= f.effectiveAt) e.expiresAt = 'Must be after effective date';
  if (f.uptimeSla !== '') {
    const n = Number(f.uptimeSla);
    if (Number.isNaN(n) || n < 0 || n > 100) e.uptimeSla = 'Must be 0–100';
  }
  return e;
}

const INITIAL_STATE: FormState = {
  name: '',
  version: '0.1.0',
  providerName: '',
  consumerNames: '',
  scopeType: 'collection',
  scopePath: '',
  effectiveAt: todayISO(),
  expiresAt: '',
  breakingChangePolicy: 'lenient',
  noticeDays: '30',
  uptimeSla: '',
};

export function NewContractModal({
  open,
  onOpenChange,
  collectionId,
  collectionName: _collectionName,
}: NewContractModalProps) {
  const createContract = useContractsStore((s) => s.createContract);
  const recomputeDrift = useContractsStore((s) => s.recomputeDrift);

  const [form, setForm] = useState<FormState>(INITIAL_STATE);
  const [errors, setErrors] = useState<FormErrors>({});
  const [warnings, setWarnings] = useState<Partial<Record<keyof FormState, string>>>({});
  const [saving, setSaving] = useState(false);

  function checkWarnings(f: FormState): Partial<Record<keyof FormState, string>> {
    const w: Partial<Record<keyof FormState, string>> = {};
    if (f.effectiveAt) {
      const d = new Date(`${f.effectiveAt}T00:00:00`);
      if (d < new Date()) {
        w.effectiveAt = 'This date is in the past — the contract will take effect immediately';
      }
    }
    return w;
  }

  function setField<K extends keyof FormState>(field: K) {
    return (e: React.ChangeEvent<HTMLInputElement>) =>
      setForm((prev) => ({ ...prev, [field]: e.target.value }));
  }

  function setEffectiveAt(e: React.ChangeEvent<HTMLInputElement>) {
    const value = e.target.value;
    setForm((prev) => ({ ...prev, effectiveAt: value }));
    setWarnings(checkWarnings({ ...form, effectiveAt: value }));
  }

  function resetAndClose() {
    setForm(INITIAL_STATE);
    setErrors({});
    setWarnings({});
    setSaving(false);
    onOpenChange(false);
  }

  async function submit(publishImmediately: boolean) {
    const errs = validate(form);
    if (Object.keys(errs).length > 0) {
      setErrors(errs);
      return;
    }
    setSaving(true);
    try {
      const scope: ContractScope =
        form.scopeType === 'collection'
          ? { type: 'collection' }
          : form.scopeType === 'folder'
            ? { type: 'folder', rel_path: form.scopePath }
            : { type: 'request', rel_path: form.scopePath };

      const policy: ContractPolicy = {
        breakingChangePolicy: form.breakingChangePolicy,
        noticeDays: Math.max(0, Number.parseInt(form.noticeDays, 10) || 30),
        uptimeSla: form.uptimeSla !== '' ? Number(form.uptimeSla) : null,
      };

      const provider: Party = {
        id: slugify(form.providerName),
        name: form.providerName.trim(),
        kind: 'team',
      };
      const consumers = buildParties(form.consumerNames);

      await createContract(
        collectionId,
        {
          name: form.name.trim(),
          version: form.version.trim(),
          provider,
          consumers,
          scope,
          policy,
          effectiveAt: form.effectiveAt,
          expiresAt: form.expiresAt || null,
          publishImmediately,
        },
        [],
      );

      if (publishImmediately) {
        await recomputeDrift(collectionId);
        toast.success('Contract created and published.');
      } else {
        toast('Contract saved as draft.');
      }

      resetAndClose();
    } catch (err) {
      setErrors({ _global: String(err) });
    } finally {
      setSaving(false);
    }
  }

  function inputProps(field: keyof FormState) {
    return {
      id: `nc-${field}`,
      value: form[field] as string,
      onChange: setField(field),
      'aria-invalid': !!errors[field],
      'aria-describedby': errors[field] ? `nc-${field}-err` : undefined,
    };
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(v) => {
        if (!saving) {
          if (!v) resetAndClose();
          else onOpenChange(v);
        }
      }}
    >
      <DialogContent className='max-w-lg max-h-[90vh] overflow-y-auto'>
        <DialogHeader>
          <DialogTitle>New contract</DialogTitle>
        </DialogHeader>

        <div className='space-y-4 py-2'>
          {/* ── Name ─────────────────────────── */}
          <div className='space-y-1.5'>
            <Label htmlFor='nc-name' className='text-sm'>
              Title{' '}
              <span className='text-destructive' aria-hidden='true'>
                *
              </span>
            </Label>
            <Input {...inputProps('name')} autoFocus placeholder='Payments API v2' />
            {errors.name && (
              <p id='nc-name-err' className='text-xs text-destructive'>
                {errors.name}
              </p>
            )}
          </div>

          {/* ── Version ──────────────────────── */}
          <div className='space-y-1.5'>
            <Label htmlFor='nc-version' className='text-sm'>
              Version
            </Label>
            <Input {...inputProps('version')} className='font-mono w-36' placeholder='0.1.0' />
            {errors.version && (
              <p id='nc-version-err' className='text-xs text-destructive'>
                {errors.version}
              </p>
            )}
          </div>

          {/* ── Provider + Consumer(s) ─────── */}
          <div className='grid grid-cols-2 gap-3'>
            <div className='space-y-1.5'>
              <Label htmlFor='nc-providerName' className='text-sm'>
                Provider team{' '}
                <span className='text-destructive' aria-hidden='true'>
                  *
                </span>
              </Label>
              <Input {...inputProps('providerName')} placeholder='Billing Team' />
              {errors.providerName && (
                <p id='nc-providerName-err' className='text-xs text-destructive'>
                  {errors.providerName}
                </p>
              )}
            </div>
            <div className='space-y-1.5'>
              <Label htmlFor='nc-consumerNames' className='text-sm'>
                Consumer team(s){' '}
                <span className='text-destructive' aria-hidden='true'>
                  *
                </span>
              </Label>
              <Input {...inputProps('consumerNames')} placeholder='Platform, Mobile' />
              <p className='text-[11px] text-muted-foreground leading-none'>Comma-separated</p>
              {errors.consumerNames && (
                <p id='nc-consumerNames-err' className='text-xs text-destructive'>
                  {errors.consumerNames}
                </p>
              )}
            </div>
          </div>

          {/* ── Scope ────────────────────────── */}
          <div className='space-y-1.5'>
            <Label className='text-sm'>Scope</Label>
            <RadioGroup
              value={form.scopeType}
              onValueChange={(v) =>
                setForm((p) => ({ ...p, scopeType: v as FormState['scopeType'] }))
              }
              className='flex gap-4'
            >
              {(['collection', 'folder', 'request'] as const).map((s) => (
                <div key={s} className='flex items-center gap-1.5'>
                  <RadioGroupItem value={s} id={`scope-${s}`} />
                  <Label
                    htmlFor={`scope-${s}`}
                    className='text-sm font-normal cursor-pointer capitalize'
                  >
                    {s}
                  </Label>
                </div>
              ))}
            </RadioGroup>
            {form.scopeType !== 'collection' && (
              <Input
                id='nc-scopePath'
                value={form.scopePath}
                onChange={setField('scopePath')}
                className='mt-1.5 font-mono text-sm'
                placeholder={form.scopeType === 'folder' ? 'auth/' : 'requests/payments.yml'}
              />
            )}
          </div>

          {/* ── Dates ────────────────────────── */}
          <div className='grid grid-cols-2 gap-3'>
            <div className='space-y-1.5'>
              <Label htmlFor='nc-effectiveAt' className='text-sm'>
                Effective date{' '}
                <span className='text-destructive' aria-hidden='true'>
                  *
                </span>
              </Label>
              <Input type='date' {...inputProps('effectiveAt')} onChange={setEffectiveAt} />
              {errors.effectiveAt && (
                <p id='nc-effectiveAt-err' className='text-xs text-destructive'>
                  {errors.effectiveAt}
                </p>
              )}
              {!errors.effectiveAt && warnings.effectiveAt && (
                <p className='text-xs text-[hsl(var(--warning))] flex items-center gap-1'>
                  <span aria-hidden='true'>⚠</span>
                  {warnings.effectiveAt}
                </p>
              )}
            </div>
            <div className='space-y-1.5'>
              <Label htmlFor='nc-expiresAt' className='text-sm'>
                Expiry date{' '}
                <span className='text-[11px] font-normal text-muted-foreground'>(optional)</span>
              </Label>
              <Input type='date' {...inputProps('expiresAt')} />
              {errors.expiresAt && (
                <p id='nc-expiresAt-err' className='text-xs text-destructive'>
                  {errors.expiresAt}
                </p>
              )}
            </div>
          </div>

          {/* ── Breaking change policy ────────── */}
          <div className='space-y-1.5'>
            <Label className='text-sm'>Breaking change policy</Label>
            <RadioGroup
              value={form.breakingChangePolicy}
              onValueChange={(v) =>
                setForm((p) => ({
                  ...p,
                  breakingChangePolicy: v as FormState['breakingChangePolicy'],
                }))
              }
              className='flex gap-4'
            >
              {(
                [
                  ['strict', 'Strict'],
                  ['lenient', 'Lenient'],
                  ['additive_ok', 'Additive OK'],
                ] as const
              ).map(([val, label]) => (
                <div key={val} className='flex items-center gap-1.5'>
                  <RadioGroupItem value={val} id={`policy-${val}`} />
                  <Label htmlFor={`policy-${val}`} className='text-sm font-normal cursor-pointer'>
                    {label}
                  </Label>
                </div>
              ))}
            </RadioGroup>
          </div>

          {/* ── Notice period + Uptime SLA ────── */}
          <div className='grid grid-cols-2 gap-3'>
            <div className='space-y-1.5'>
              <Label htmlFor='nc-noticeDays' className='text-sm'>
                Notice period
              </Label>
              <div className='flex items-center gap-2'>
                <Input
                  id='nc-noticeDays'
                  type='number'
                  min={0}
                  max={365}
                  value={form.noticeDays}
                  onChange={setField('noticeDays')}
                  className='w-20'
                />
                <span className='text-sm text-muted-foreground'>days</span>
              </div>
            </div>
            <div className='space-y-1.5'>
              <Label htmlFor='nc-uptimeSla' className='text-sm'>
                Uptime SLA{' '}
                <span className='text-[11px] font-normal text-muted-foreground'>(optional)</span>
              </Label>
              <div className='flex items-center gap-2'>
                <Input
                  id='nc-uptimeSla'
                  type='number'
                  min={0}
                  max={100}
                  step={0.1}
                  value={form.uptimeSla}
                  onChange={setField('uptimeSla')}
                  className='w-24'
                  placeholder='99.9'
                  aria-invalid={!!errors.uptimeSla}
                />
                <span className='text-sm text-muted-foreground'>%</span>
              </div>
              {errors.uptimeSla && <p className='text-xs text-destructive'>{errors.uptimeSla}</p>}
            </div>
          </div>

          {/* ── Global error ─────────────────── */}
          {errors._global && (
            <p className='text-xs text-destructive bg-destructive/10 rounded-md px-3 py-2'>
              {errors._global}
            </p>
          )}
        </div>

        <DialogFooter className='gap-2 pt-2 flex-wrap'>
          <Button variant='ghost' onClick={resetAndClose} disabled={saving}>
            Cancel
          </Button>
          <Button variant='outline' onClick={() => submit(false)} disabled={saving}>
            {saving ? 'Saving…' : 'Save as Draft'}
          </Button>
          <Button onClick={() => submit(true)} disabled={saving}>
            {saving ? 'Creating…' : 'Create & Publish →'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
