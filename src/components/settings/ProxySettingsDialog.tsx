import { Loader2 } from 'lucide-react';
import { useEffect, useState } from 'react';
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
import {
  getProxySettings,
  type ProxyMode,
  type ProxyPasswordChange,
  saveProxySettings,
} from '@/lib/tauri-api';

interface ProxySettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const MODES: { value: ProxyMode; label: string; hint: string }[] = [
  {
    value: 'system',
    label: 'System',
    hint: 'Use the HTTP_PROXY, HTTPS_PROXY and NO_PROXY variables.',
  },
  { value: 'none', label: 'None', hint: 'Always connect directly.' },
  {
    value: 'custom',
    label: 'Custom',
    hint: 'Use the proxy URLs below. A single URL is used for both HTTP and HTTPS.',
  },
];

export function ProxySettingsDialog({ open, onOpenChange }: ProxySettingsDialogProps) {
  const [loading, setLoading] = useState(true);
  const [loadFailed, setLoadFailed] = useState(false);
  const [saving, setSaving] = useState(false);
  const [mode, setMode] = useState<ProxyMode>('system');
  const [httpProxy, setHttpProxy] = useState('');
  const [httpsProxy, setHttpsProxy] = useState('');
  const [noProxy, setNoProxy] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [hasPassword, setHasPassword] = useState(false);
  const [removePassword, setRemovePassword] = useState(false);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    setLoadFailed(false);
    getProxySettings()
      .then((s) => {
        if (cancelled) return;
        setMode(s.mode);
        setHttpProxy(s.httpProxy ?? '');
        setHttpsProxy(s.httpsProxy ?? '');
        setNoProxy(s.noProxy ?? '');
        setUsername(s.username ?? '');
        setHasPassword(s.hasPassword);
        setPassword('');
        setRemovePassword(false);
      })
      .catch(() => {
        if (cancelled) return;
        // Saving now would overwrite the stored setting with the defaults shown.
        setLoadFailed(true);
        toast.error('Could not load the proxy settings');
      })
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [open]);

  const handleSave = async () => {
    const change: ProxyPasswordChange = password
      ? { action: 'set', value: password }
      : removePassword
        ? { action: 'clear' }
        : { action: 'keep' };
    setSaving(true);
    try {
      await saveProxySettings({ mode, httpProxy, httpsProxy, noProxy, username }, change);
      toast.success('Proxy settings saved');
      onOpenChange(false);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>Proxy</DialogTitle>
        </DialogHeader>
        {loading ? (
          <div className='flex justify-center p-6'>
            <Loader2 className='h-4 w-4 animate-spin' aria-label='Loading' />
          </div>
        ) : loadFailed ? (
          <p role='alert' className='text-sm text-destructive'>
            Could not load the proxy settings. Close this dialog and try again.
          </p>
        ) : (
          <div className='space-y-4'>
            <RadioGroup value={mode} onValueChange={(v) => setMode(v as ProxyMode)}>
              {MODES.map((m) => (
                <div key={m.value} className='flex items-start gap-2'>
                  <RadioGroupItem value={m.value} id={`proxy-${m.value}`} className='mt-0.5' />
                  <Label htmlFor={`proxy-${m.value}`} className='flex flex-col gap-0.5'>
                    <span>{m.label}</span>
                    <span className='text-xs font-normal text-muted-foreground'>{m.hint}</span>
                  </Label>
                </div>
              ))}
            </RadioGroup>
            {mode === 'custom' && (
              <div className='space-y-3'>
                <div className='space-y-1.5'>
                  <Label htmlFor='proxy-http'>HTTP proxy URL</Label>
                  <Input
                    id='proxy-http'
                    aria-label='HTTP proxy URL'
                    placeholder='http://proxy.corp:8080'
                    value={httpProxy}
                    onChange={(e) => setHttpProxy(e.target.value)}
                  />
                </div>
                <div className='space-y-1.5'>
                  <Label htmlFor='proxy-https'>HTTPS proxy URL</Label>
                  <Input
                    id='proxy-https'
                    aria-label='HTTPS proxy URL'
                    placeholder='http://proxy.corp:8080'
                    value={httpsProxy}
                    onChange={(e) => setHttpsProxy(e.target.value)}
                  />
                </div>
                <div className='space-y-1.5'>
                  <Label htmlFor='proxy-no'>No proxy for</Label>
                  <Input
                    id='proxy-no'
                    aria-label='No proxy for'
                    placeholder='localhost, .internal.corp'
                    value={noProxy}
                    onChange={(e) => setNoProxy(e.target.value)}
                  />
                </div>
                <div className='grid grid-cols-2 gap-2'>
                  <div className='space-y-1.5'>
                    <Label htmlFor='proxy-user'>Username</Label>
                    <Input
                      id='proxy-user'
                      aria-label='Proxy username'
                      value={username}
                      onChange={(e) => setUsername(e.target.value)}
                    />
                  </div>
                  <div className='space-y-1.5'>
                    <Label htmlFor='proxy-pass'>Password</Label>
                    <Input
                      id='proxy-pass'
                      aria-label='Proxy password'
                      type='password'
                      autoComplete='new-password'
                      placeholder={hasPassword && !removePassword ? 'Unchanged' : ''}
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                    />
                  </div>
                </div>
                {hasPassword && !removePassword && (
                  <p className='flex items-center gap-2 text-xs text-muted-foreground'>
                    A password is saved in the system keychain.
                    <Button
                      variant='link'
                      size='sm'
                      className='h-auto p-0 text-xs'
                      onClick={() => {
                        // Removing means no password at all, so a typed one is dropped too.
                        setPassword('');
                        setRemovePassword(true);
                      }}
                    >
                      Remove saved password
                    </Button>
                  </p>
                )}
                <p className='text-xs text-muted-foreground'>
                  OAuth 2.0 token requests (token, refresh and authorization-code exchange) also use
                  this setting.
                </p>
              </div>
            )}
          </div>
        )}
        <DialogFooter>
          <Button variant='ghost' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={loading || saving || loadFailed}>
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
