import { open as openFilePicker } from '@tauri-apps/plugin-dialog';
import { FolderOpen } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { GitCredentials } from '@/lib/tauri-api';
import { getDefaultSshKeyPath, listSshKeyPaths, loadGitCredentials, saveGitCredentials } from '@/lib/tauri-api';
import { useGitStore } from '@/stores/git-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

type AuthType = 'sshKey' | 'sshAgent' | 'userPass' | 'token';

export function GitCredentialsDialog() {
  const { showCredentialsDialog, setShowCredentialsDialog, setCredentials } = useGitStore();
  const [authType, setAuthType] = useState<AuthType>('sshKey');
  const [privateKeyPath, setPrivateKeyPath] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [token, setToken] = useState('');
  const [saveError, setSaveError] = useState<string | null>(null);
  const [availableKeyPaths, setAvailableKeyPaths] = useState<string[]>([]);

  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);

  // On open: load persisted credentials first; fall back to SSH key auto-detection.
  useEffect(() => {
    if (!showCredentialsDialog) return;
    setSaveError(null);

    (async () => {
      // Load all available SSH keys in ~/.ssh/ for the picker.
      try {
        const keys = await listSshKeyPaths();
        setAvailableKeyPaths(keys);
      } catch {
        setAvailableKeyPaths([]);
      }

      try {
        const saved = await loadGitCredentials(activeWorkspaceId);
        if (saved) {
          if (saved.type === 'sshKey') {
            setAuthType('sshKey');
            setPrivateKeyPath(saved.privateKeyPath);
            setPassphrase(saved.passphrase ?? '');
          } else if (saved.type === 'sshAgent') {
            setAuthType('sshAgent');
          } else if (saved.type === 'userPass') {
            setAuthType('userPass');
            setUsername(saved.username);
            setPassword(saved.password);
          } else if (saved.type === 'token') {
            setAuthType('token');
            setToken(saved.token);
          }
          return;
        }
      } catch {
        // Keychain unavailable — fall through to defaults.
      }

      // No saved credentials: auto-detect default SSH key.
      try {
        const detected = await getDefaultSshKeyPath();
        if (detected) setPrivateKeyPath(detected);
      } catch {
        // Auto-detection failed — leave field empty (placeholder shown).
      }
    })();
  }, [showCredentialsDialog]);

  const handleBrowseKey = async () => {
    // Always open the picker in ~/.ssh/ — use getDefaultSshKeyPath to resolve
    // the home directory cross-platform, then strip the filename.
    let sshDir: string | undefined;
    try {
      const detected = await getDefaultSshKeyPath();
      if (detected) {
        sshDir = detected.replace(/[\\/][^\\/]+$/, '');
      }
    } catch {
      sshDir = undefined;
    }
    const selected = await openFilePicker({ multiple: false, defaultPath: sshDir });
    if (typeof selected === 'string' && selected) {
      setPrivateKeyPath(selected);
    }
  };

  const handleConnect = async () => {
    let creds: GitCredentials;
    switch (authType) {
      case 'sshKey':
        creds = { type: 'sshKey', privateKeyPath, passphrase: passphrase || undefined };
        break;
      case 'sshAgent':
        creds = { type: 'sshAgent' };
        break;
      case 'userPass':
        creds = { type: 'userPass', username, password };
        break;
      case 'token':
        creds = { type: 'token', token };
        break;
    }

    // Persist to OS keychain; surface error inline but never block the connect.
    try {
      await saveGitCredentials(activeWorkspaceId, creds);
    } catch (e) {
      setSaveError(`Could not save credentials to keychain: ${String(e)}`);
    }

    setCredentials(creds);
  };

  return (
    <Dialog open={showCredentialsDialog} onOpenChange={setShowCredentialsDialog}>
      <DialogContent className='w-auto min-w-[24rem] max-w-[min(90vw,_42rem)]'>
        <DialogHeader>
          <DialogTitle>Git Credentials</DialogTitle>
        </DialogHeader>

        <div className='space-y-3'>
          {/* Save error alert */}
          {saveError && (
            <p role='alert' className='text-xs text-destructive'>
              {saveError}
            </p>
          )}

          {/* Authentication type */}
          <div>
            <Label htmlFor='auth-type-select' className='text-sm'>
              Authentication Type
            </Label>
            <Select value={authType} onValueChange={(v) => setAuthType(v as AuthType)}>
              <SelectTrigger id='auth-type-select' className='h-8 text-sm'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='sshAgent'>SSH Agent</SelectItem>
                <SelectItem value='sshKey'>SSH Key</SelectItem>
                <SelectItem value='userPass'>Username / Password</SelectItem>
                <SelectItem value='token'>Token</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {/* SSH Key fields */}
          {authType === 'sshKey' && (
            <>
              <div>
                <Label htmlFor='ssh-key-select' className='text-sm'>
                  Private Key
                </Label>
                {availableKeyPaths.length > 0 ? (
                  <Select
                    value={privateKeyPath}
                    onValueChange={async (val) => {
                      if (val === '__browse__') {
                        await handleBrowseKey();
                      } else {
                        setPrivateKeyPath(val);
                      }
                    }}
                  >
                    <SelectTrigger id='ssh-key-select' className='h-8 text-sm font-mono'>
                      <SelectValue placeholder='Select a key…' />
                    </SelectTrigger>
                    <SelectContent>
                      {/* Show all detected keys; if the current path isn't in the list add it */}
                      {[
                        ...availableKeyPaths,
                        ...(privateKeyPath && !availableKeyPaths.includes(privateKeyPath)
                          ? [privateKeyPath]
                          : []),
                      ].map((keyPath) => (
                        <SelectItem key={keyPath} value={keyPath} className='font-mono text-xs'>
                          {keyPath.replace(/.*[\\/]/, '')}
                          <span className='ml-2 font-sans text-[10px] text-muted-foreground'>
                            {keyPath.replace(/[\\/][^\\/]+$/, '')}
                          </span>
                        </SelectItem>
                      ))}
                      <SelectItem value='__browse__' className='text-xs text-muted-foreground'>
                        <FolderOpen className='mr-1.5 inline h-3 w-3' aria-hidden='true' />
                        Browse for another key…
                      </SelectItem>
                    </SelectContent>
                  </Select>
                ) : (
                  <Button
                    id='ssh-key-select'
                    variant='outline'
                    size='sm'
                    className='h-8 w-full justify-between font-mono text-xs'
                    onClick={handleBrowseKey}
                  >
                    <span>{privateKeyPath || 'Click to select a key file…'}</span>
                    <FolderOpen className='ml-2 h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />
                  </Button>
                )}
              </div>

              <div>
                <Label htmlFor='passphrase-input' className='text-sm'>
                  Passphrase <span className='text-muted-foreground'>(optional)</span>
                </Label>
                <Input
                  id='passphrase-input'
                  type='password'
                  value={passphrase}
                  onChange={(e) => setPassphrase(e.target.value)}
                  className='h-8 text-sm'
                  autoComplete='current-password'
                />
              </div>
            </>
          )}

          {/* Username / Password fields */}
          {authType === 'userPass' && (
            <>
              <div>
                <Label htmlFor='username-input' className='text-sm'>
                  Username
                </Label>
                <Input
                  id='username-input'
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  className='h-8 text-sm'
                  autoComplete='username'
                />
              </div>
              <div>
                <Label htmlFor='password-input' className='text-sm'>
                  Password
                </Label>
                <Input
                  id='password-input'
                  type='password'
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  className='h-8 text-sm'
                  autoComplete='current-password'
                />
              </div>
            </>
          )}

          {/* Token field */}
          {authType === 'token' && (
            <div>
              <Label htmlFor='token-input' className='text-sm'>
                Token
              </Label>
              <Input
                id='token-input'
                type='password'
                value={token}
                onChange={(e) => setToken(e.target.value)}
                className='h-8 text-sm'
                autoComplete='current-password'
              />
            </div>
          )}

          <Button onClick={handleConnect} className='w-full' size='sm'>
            Connect
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
