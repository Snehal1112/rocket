import { useState } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { gitClone } from '@/lib/tauri-api';
import type { GitCredentials } from '@/lib/tauri-api';

export function GitCloneDialog() {
  const [open, setOpen] = useState(false);
  const [url, setUrl] = useState('');
  const [destPath, setDestPath] = useState('');
  const [authType, setAuthType] = useState<string>('sshAgent');
  const [token, setToken] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [privateKeyPath, setPrivateKeyPath] = useState('~/.ssh/id_rsa');
  const [cloning, setCloning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Auto-generate dest path from URL.
  const handleUrlChange = (value: string) => {
    setUrl(value);
    const match = value.match(/\/([^/]+?)(?:\.git)?$/);
    if (match) {
      setDestPath(`~/.rocket-api/collections/${match[1]}`);
    }
  };

  const handleClone = async () => {
    if (!url.trim() || !destPath.trim()) return;
    let creds: GitCredentials;
    switch (authType) {
      case 'sshKey':
        creds = { type: 'sshKey', privateKeyPath, passphrase: undefined };
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
      default:
        creds = { type: 'sshAgent' };
    }
    setCloning(true);
    setError(null);
    try {
      await gitClone(url.trim(), destPath.trim(), creds);
      setOpen(false);
      setUrl('');
      setDestPath('');
    } catch (e) {
      setError(String(e));
    } finally {
      setCloning(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm" className="text-xs">Clone from Git</Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Clone Repository</DialogTitle>
        </DialogHeader>
        <div className="space-y-3">
          <div>
            <Label className="text-xs">Repository URL</Label>
            <Input
              placeholder="https://github.com/..."
              value={url}
              onChange={(e) => handleUrlChange(e.target.value)}
              className="h-8 text-xs"
            />
          </div>
          <div>
            <Label className="text-xs">Destination Path</Label>
            <Input
              value={destPath}
              onChange={(e) => setDestPath(e.target.value)}
              className="h-8 text-xs"
            />
          </div>
          <div>
            <Label className="text-xs">Authentication</Label>
            <Select value={authType} onValueChange={setAuthType}>
              <SelectTrigger className="h-8 text-xs"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="sshAgent">SSH Agent</SelectItem>
                <SelectItem value="sshKey">SSH Key</SelectItem>
                <SelectItem value="userPass">Username / Password</SelectItem>
                <SelectItem value="token">Token</SelectItem>
              </SelectContent>
            </Select>
          </div>
          {authType === 'token' && (
            <div>
              <Label className="text-xs">Token</Label>
              <Input
                type="password"
                value={token}
                onChange={(e) => setToken(e.target.value)}
                className="h-8 text-xs"
              />
            </div>
          )}
          {authType === 'userPass' && (
            <>
              <div>
                <Label className="text-xs">Username</Label>
                <Input value={username} onChange={(e) => setUsername(e.target.value)} className="h-8 text-xs" />
              </div>
              <div>
                <Label className="text-xs">Password</Label>
                <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} className="h-8 text-xs" />
              </div>
            </>
          )}
          {authType === 'sshKey' && (
            <div>
              <Label className="text-xs">Private Key Path</Label>
              <Input value={privateKeyPath} onChange={(e) => setPrivateKeyPath(e.target.value)} className="h-8 text-xs" />
            </div>
          )}
          {error && <p className="text-xs text-destructive">{error}</p>}
          <Button onClick={handleClone} disabled={cloning || !url.trim()} className="w-full" size="sm">
            {cloning ? 'Cloning...' : 'Clone'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
