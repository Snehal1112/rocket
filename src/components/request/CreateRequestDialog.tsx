import { useState } from 'react';
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { sanitizeFilename } from '@/lib/filename-utils';
import { createDefaultRequest } from '@/lib/pane-utils';
import { saveRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { HttpMethod, RequestTab } from '@/types/pane-types';

type RequestType = 'http' | 'graphql' | 'grpc' | 'websocket' | 'curl';

const REQUEST_TYPES: { label: string; value: RequestType }[] = [
  { label: 'HTTP', value: 'http' },
  { label: 'GraphQL', value: 'graphql' },
  { label: 'gRPC', value: 'grpc' },
  { label: 'WebSocket', value: 'websocket' },
  { label: 'From cURL', value: 'curl' },
];

const HTTP_METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'HEAD'];

export interface CreateRequestDialogProps {
  open: boolean;
  collectionName: string;
  folderPath?: string;
  onClose: () => void;
}

export function CreateRequestDialog({
  open,
  collectionName,
  folderPath,
  onClose,
}: CreateRequestDialogProps) {
  const [requestType, setRequestType] = useState<RequestType>('http');
  const [name, setName] = useState('');
  const [method, setMethod] = useState<HttpMethod>('GET');
  const [url, setUrl] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');

  const trimmedName = name.trim();
  const fsName = trimmedName ? sanitizeFilename(trimmedName) : '';

  // Show filesystem name hint when unsafe chars were replaced.
  const showFsHint = fsName !== '' && fsName !== `${trimmedName}.yml`;

  function reset() {
    setName('');
    setUrl('');
    setMethod('GET');
    setRequestType('http');
    setError('');
  }

  async function handleCreate() {
    if (!trimmedName) {
      setError('Request name is required.');
      return;
    }
    setSaving(true);
    setError('');
    try {
      const uid = crypto.randomUUID();
      const filePath = folderPath ? `${folderPath}/${fsName}` : fsName;
      const payload = {
        uid,
        name: trimmedName,
        method,
        url,
        headers: [],
        auth: { authType: 'none' as const },
        fileName: filePath,
      };
      const saved = await saveRequest(collectionName, filePath, payload);
      const tab: RequestTab = {
        id: uid,
        title: trimmedName,
        tabType: 'request',
        request: {
          ...createDefaultRequest(),
          method,
          url,
          requestType: requestType === 'curl' ? 'http' : requestType,
        },
        response: null,
        isDirty: false,
        source: { collection: collectionName, path: saved.fileName ?? filePath },
      };
      usePaneStore.getState().openTab(tab);
      reset();
      onClose();
    } catch (err) {
      console.error('[CreateRequestDialog] failed:', err);
      setError('Failed to create request. Please try again.');
    } finally {
      setSaving(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === 'Enter' && !saving) void handleCreate();
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(v) => {
        if (!v) {
          reset();
          onClose();
        }
      }}
    >
      <DialogContent className='sm:max-w-md' onKeyDown={handleKeyDown}>
        <DialogHeader>
          <DialogTitle>New Request</DialogTitle>
        </DialogHeader>
        <div className='flex flex-col gap-4 py-2'>
          <div className='flex flex-col gap-1.5'>
            <Label htmlFor='crd-type' className='text-xs font-medium'>
              Request Type
            </Label>
            <Select value={requestType} onValueChange={(v) => setRequestType(v as RequestType)}>
              <SelectTrigger id='crd-type' className='h-9'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {REQUEST_TYPES.map((t) => (
                  <SelectItem key={t.value} value={t.value}>
                    {t.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className='flex flex-col gap-1.5'>
            <Label htmlFor='crd-name' className='text-xs font-medium'>
              Request Name
            </Label>
            <Input
              id='crd-name'
              autoFocus
              placeholder='e.g. GET /users/:id'
              value={name}
              onChange={(e) => {
                setName(e.target.value);
                setError('');
              }}
              className='h-9'
            />
            {showFsHint && (
              <p className='text-xs text-muted-foreground'>
                Saved as: <span className='font-mono'>{fsName}</span>
              </p>
            )}
          </div>
          {(requestType === 'http' || requestType === 'curl') && (
            <div className='flex flex-col gap-1.5'>
              <Label htmlFor='crd-method' className='text-xs font-medium'>
                HTTP Method
              </Label>
              <Select value={method} onValueChange={(v) => setMethod(v as HttpMethod)}>
                <SelectTrigger id='crd-method' className='h-9'>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {HTTP_METHODS.map((m) => (
                    <SelectItem key={m} value={m}>
                      {m}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}
          <div className='flex flex-col gap-1.5'>
            <Label htmlFor='crd-url' className='text-xs font-medium'>
              URL
            </Label>
            <Input
              id='crd-url'
              placeholder='https://api.example.com/users'
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              className='h-9 font-mono text-sm'
            />
          </div>
          {error && <p className='text-xs text-destructive'>{error}</p>}
        </div>
        <DialogFooter>
          <Button
            variant='outline'
            size='sm'
            onClick={() => {
              reset();
              onClose();
            }}
            disabled={saving}
          >
            Cancel
          </Button>
          <Button size='sm' onClick={() => void handleCreate()} disabled={saving || !trimmedName}>
            {saving ? 'Creating...' : 'Create'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
