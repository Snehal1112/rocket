import { open as openFilePicker } from '@tauri-apps/plugin-dialog';
import { Paperclip, Plus, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';

export interface ContractFormValues {
  title: string;
  provider: string;
  consumer: string;
  project: string;
  version: string;
  effectiveDate: string;
  expiryDate: string;
  scopeType: 'collection' | 'folder' | 'request';
  scopePath: string;
  /** Relative paths already stored in the collection (populated when editing). */
  existingDocumentPaths: string[];
  /** Absolute paths newly chosen via the file picker (not yet copied). */
  newDocumentPaths: string[];
}

interface ContractFormProps {
  values: ContractFormValues;
  onChange: (values: ContractFormValues) => void;
  folders: string[];
  requests: string[];
  error: string | null;
}

export function ContractForm({ values, onChange, folders, requests, error }: ContractFormProps) {
  const set = (field: keyof ContractFormValues) => (e: React.ChangeEvent<HTMLInputElement>) =>
    onChange({ ...values, [field]: e.target.value });

  const handleBrowseDocument = async () => {
    const selected = await openFilePicker({ multiple: true });
    const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
    if (paths.length > 0) {
      onChange({ ...values, newDocumentPaths: [...values.newDocumentPaths, ...paths] });
    }
  };

  const handleRemoveNew = (path: string) => {
    onChange({
      ...values,
      newDocumentPaths: values.newDocumentPaths.filter((p) => p !== path),
    });
  };

  const handleRemoveExisting = (path: string) => {
    onChange({
      ...values,
      existingDocumentPaths: values.existingDocumentPaths.filter((p) => p !== path),
    });
  };

  const setScopeType = (v: ContractFormValues['scopeType']) =>
    onChange({ ...values, scopeType: v, scopePath: '' });

  return (
    <div className='flex flex-col gap-4 overflow-y-auto pr-2'>
      {/* Validation error */}
      {error && (
        <p className='text-xs text-destructive bg-destructive/10 rounded-md px-3 py-2'>{error}</p>
      )}

      {/* Title */}
      <div className='space-y-1.5'>
        <Label htmlFor='cl-title' className='text-xs'>
          Contract title
        </Label>
        <Input
          id='cl-title'
          placeholder='Payments API v2.3'
          value={values.title}
          onChange={set('title')}
          className='h-8 text-sm'
        />
      </div>

      {/* Provider + Consumer */}
      <div className='grid grid-cols-2 gap-3'>
        <div className='space-y-1.5'>
          <Label htmlFor='cl-provider' className='text-xs'>
            Provider team
          </Label>
          <Input
            id='cl-provider'
            placeholder='Billing Team'
            value={values.provider}
            onChange={set('provider')}
            className='h-8 text-sm'
          />
        </div>
        <div className='space-y-1.5'>
          <Label htmlFor='cl-consumer' className='text-xs'>
            Consumer team
          </Label>
          <Input
            id='cl-consumer'
            placeholder='Platform Team'
            value={values.consumer}
            onChange={set('consumer')}
            className='h-8 text-sm'
          />
        </div>
      </div>

      {/* Project + Version */}
      <div className='grid grid-cols-2 gap-3'>
        <div className='space-y-1.5'>
          <Label htmlFor='cl-project' className='text-xs'>
            Project
          </Label>
          <Input
            id='cl-project'
            placeholder='Checkout Revamp'
            value={values.project}
            onChange={set('project')}
            className='h-8 text-sm'
          />
        </div>
        <div className='space-y-1.5'>
          <Label htmlFor='cl-version' className='text-xs'>
            Version
          </Label>
          <Input
            id='cl-version'
            placeholder='v1.0'
            value={values.version}
            onChange={set('version')}
            className='h-8 text-sm'
          />
        </div>
      </div>

      {/* Effective + Expiry */}
      <div className='grid grid-cols-2 gap-3'>
        <div className='space-y-1.5'>
          <Label htmlFor='cl-effective' className='text-xs'>
            Effective date
          </Label>
          <Input
            id='cl-effective'
            type='date'
            value={values.effectiveDate}
            onChange={set('effectiveDate')}
            className='h-8 text-sm'
          />
        </div>
        <div className='space-y-1.5'>
          <Label htmlFor='cl-expiry' className='text-xs'>
            Expiry (optional)
          </Label>
          <Input
            id='cl-expiry'
            type='date'
            value={values.expiryDate}
            onChange={set('expiryDate')}
            className='h-8 text-sm'
          />
        </div>
      </div>

      {/* Scope */}
      <div className='space-y-2'>
        <Label className='text-xs'>Scope</Label>
        <RadioGroup
          value={values.scopeType}
          onValueChange={(v) => setScopeType(v as ContractFormValues['scopeType'])}
          className='space-y-1.5'
        >
          {/* Collection */}
          <div className='flex items-center gap-2'>
            <RadioGroupItem value='collection' id='scope-col' />
            <Label htmlFor='scope-col' className='text-xs font-normal cursor-pointer'>
              Entire collection
            </Label>
          </div>

          {/* Folder */}
          <div className='flex items-center gap-2 flex-wrap'>
            <RadioGroupItem value='folder' id='scope-folder' />
            <Label htmlFor='scope-folder' className='text-xs font-normal cursor-pointer'>
              Folder
            </Label>
            {values.scopeType === 'folder' && (
              <Select
                value={values.scopePath}
                onValueChange={(v) => onChange({ ...values, scopePath: v })}
              >
                <SelectTrigger className='h-7 text-xs w-44'>
                  <SelectValue placeholder='Select folder…' />
                </SelectTrigger>
                <SelectContent>
                  {folders.length === 0 && (
                    <SelectItem value='__none__' disabled className='text-xs text-muted-foreground'>
                      No folders found
                    </SelectItem>
                  )}
                  {folders.map((f) => (
                    <SelectItem key={f} value={f} className='text-xs'>
                      {f}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          </div>

          {/* Request */}
          <div className='flex items-center gap-2 flex-wrap'>
            <RadioGroupItem value='request' id='scope-req' />
            <Label htmlFor='scope-req' className='text-xs font-normal cursor-pointer'>
              Single request
            </Label>
            {values.scopeType === 'request' && (
              <Select
                value={values.scopePath}
                onValueChange={(v) => onChange({ ...values, scopePath: v })}
              >
                <SelectTrigger className='h-7 text-xs w-52'>
                  <SelectValue placeholder='Select request…' />
                </SelectTrigger>
                <SelectContent>
                  {requests.length === 0 && (
                    <SelectItem value='__none__' disabled className='text-xs text-muted-foreground'>
                      No requests found
                    </SelectItem>
                  )}
                  {requests.map((r) => (
                    <SelectItem key={r} value={r} className='text-xs'>
                      {r}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          </div>
        </RadioGroup>
      </div>

      {/* Document attachments */}
      <div className='space-y-1.5'>
        <Label className='text-xs'>Attach documents (optional, max 2 MB each)</Label>

        {/* Existing attachments (already stored in collection) */}
        {values.existingDocumentPaths.map((p) => (
          <div key={p} className='flex items-center gap-1'>
            <span className='inline-flex items-center gap-1.5 flex-1 h-8 px-2 rounded-md border border-border text-xs overflow-hidden'>
              <Paperclip className='h-3 w-3 shrink-0 text-muted-foreground' />
              <span className='truncate'>{p.split('/').pop() ?? p}</span>
            </span>
            <Button
              type='button'
              variant='ghost'
              size='sm'
              className='h-8 w-8 p-0 shrink-0'
              onClick={() => handleRemoveExisting(p)}
              aria-label='Remove attachment'
            >
              <X className='h-3.5 w-3.5' />
            </Button>
          </div>
        ))}

        {/* Newly picked attachments (not yet copied) */}
        {values.newDocumentPaths.map((p) => (
          <div key={p} className='flex items-center gap-1'>
            <span className='inline-flex items-center gap-1.5 flex-1 h-8 px-2 rounded-md border border-primary/40 text-xs overflow-hidden bg-primary/5'>
              <Paperclip className='h-3 w-3 shrink-0 text-primary' />
              <span className='truncate'>{p.split('/').pop() ?? p}</span>
            </span>
            <Button
              type='button'
              variant='ghost'
              size='sm'
              className='h-8 w-8 p-0 shrink-0'
              onClick={() => handleRemoveNew(p)}
              aria-label='Remove attachment'
            >
              <X className='h-3.5 w-3.5' />
            </Button>
          </div>
        ))}

        {/* Add more button */}
        <Button
          type='button'
          variant='outline'
          size='sm'
          className='h-8 w-full justify-start text-xs text-muted-foreground font-normal'
          onClick={handleBrowseDocument}
        >
          <Plus className='h-3.5 w-3.5 mr-2 shrink-0' />
          Add document…
        </Button>
      </div>
    </div>
  );
}
