import { useState } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { useGitStore } from '@/stores/git-store';
import type { GitCredentials } from '@/lib/tauri-api';

type AuthType = 'sshKey' | 'sshAgent' | 'userPass' | 'token';

export function GitCredentialsDialog() {
  const { showCredentialsDialog, setShowCredentialsDialog, setCredentials } = useGitStore();
  const [authType, setAuthType] = useState<AuthType>('sshAgent');
  const [privateKeyPath, setPrivateKeyPath] = useState('~/.ssh/id_rsa');
  const [passphrase, setPassphrase] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [token, setToken] = useState('');

  const handleConnect = () => {
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
    setCredentials(creds);
  };

  return (
    <Dialog open={showCredentialsDialog} onOpenChange={setShowCredentialsDialog}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>Git Credentials</DialogTitle>
        </DialogHeader>
        <div className="space-y-3">
          <div>
            <Label className="text-xs">Authentication Type</Label>
            <Select value={authType} onValueChange={(v) => setAuthType(v as AuthType)}>
              <SelectTrigger className="h-8 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="sshAgent">SSH Agent</SelectItem>
                <SelectItem value="sshKey">SSH Key</SelectItem>
                <SelectItem value="userPass">Username / Password</SelectItem>
                <SelectItem value="token">Token</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {authType === 'sshKey' && (
            <>
              <div>
                <Label className="text-xs">Private Key Path</Label>
                <Input value={privateKeyPath} onChange={(e) => setPrivateKeyPath(e.target.value)} className="h-8 text-xs" />
              </div>
              <div>
                <Label className="text-xs">Passphrase (optional)</Label>
                <Input type="password" value={passphrase} onChange={(e) => setPassphrase(e.target.value)} className="h-8 text-xs" />
              </div>
            </>
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

          {authType === 'token' && (
            <div>
              <Label className="text-xs">Token</Label>
              <Input type="password" value={token} onChange={(e) => setToken(e.target.value)} className="h-8 text-xs" />
            </div>
          )}

          <Button onClick={handleConnect} className="w-full" size="sm">Connect</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
