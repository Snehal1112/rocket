# OAuth2 Frontend Redesign — Plan E: Sub-Components

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the 6 OAuth2 sub-components that make up the Bruno-style section layout: TokenDisplay, ConfigSection, TokenSection, AdvancedSection, SettingsSection, AdditionalParams. Read the frontend-design SKILL.md before writing any component code.

**Architecture:** Each component receives `oauth2` state + `patchOAuth2` callback + optional variableContext. Components use shadcn/ui primitives exclusively. All live under `src/components/request/oauth2/`.

**Tech Stack:** React 18, TypeScript, shadcn/ui, Tailwind, Lucide React icons

**Spec:** `docs/superpowers/specs/2026-04-21-oauth2-frontend-redesign-design.md`

**Prerequisite:** Plan D complete (types extended, tauri-api functions added).

**IMPORTANT:** Before writing any component code, read `/mnt/skills/public/frontend-design/SKILL.md` for design tokens, component patterns, and styling constraints.

---

## File Map

| File | Action | Purpose |
|---|---|---|
| `src/components/request/oauth2/OAuth2TokenDisplay.tsx` | Create | Collapsible Access Token + ID Token panels |
| `src/components/request/oauth2/OAuth2ConfigSection.tsx` | Create | Configuration: URLs, credentials, scope, PKCE, system browser |
| `src/components/request/oauth2/OAuth2TokenSection.tsx` | Create | Token Source, Token ID, Add token to, Header Prefix |
| `src/components/request/oauth2/OAuth2AdvancedSection.tsx` | Create | Refresh Token URL |
| `src/components/request/oauth2/OAuth2SettingsSection.tsx` | Create | Auto-fetch, auto-refresh checkboxes |
| `src/components/request/oauth2/OAuth2AdditionalParams.tsx` | Create | Tabbed key-value editor (Authorization/Token/Refresh) |

---

### Task 1: OAuth2TokenDisplay + OAuth2SettingsSection + OAuth2AdvancedSection

These three are the simplest components. Building them first establishes the pattern.

**Files:**
- Create: `src/components/request/oauth2/OAuth2TokenDisplay.tsx`
- Create: `src/components/request/oauth2/OAuth2SettingsSection.tsx`
- Create: `src/components/request/oauth2/OAuth2AdvancedSection.tsx`

- [ ] **Step 1: Read the frontend-design skill**

```bash
cat /mnt/skills/public/frontend-design/SKILL.md
```

Follow its design tokens and component patterns for all components.

- [ ] **Step 2: Create OAuth2TokenDisplay.tsx**

This component shows collapsible panels for Access Token and ID Token when tokens exist.

```tsx
import { ChevronDown, ChevronRight, Copy, Key } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { Input } from '@/components/ui/input';
import type { AuthState, OAuth2JwtClaims } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;

interface OAuth2TokenDisplayProps {
  oauth2: OAuth2State;
}

function tokenExpiryDisplay(expiresIn: number | null, acquiredAt: number | null): string {
  if (!expiresIn || !acquiredAt) return 'No expiry';
  const expiresAt = acquiredAt + expiresIn;
  const now = Math.floor(Date.now() / 1000);
  if (now >= expiresAt) return 'Expired';
  const remaining = expiresAt - now;
  const date = new Date(expiresAt * 1000);
  const time = date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (remaining < 60) return `Expires in ${remaining}s (at ${time})`;
  if (remaining < 3600) return `Expires in ${Math.floor(remaining / 60)}m (at ${time})`;
  return `Expires in ${Math.floor(remaining / 3600)}h ${Math.floor((remaining % 3600) / 60)}m`;
}

function isExpired(expiresIn: number | null, acquiredAt: number | null): boolean {
  if (!expiresIn || !acquiredAt) return false;
  return Math.floor(Date.now() / 1000) >= acquiredAt + expiresIn;
}

function formatTimestamp(ts: number | null): string {
  if (!ts) return '—';
  return new Date(ts * 1000).toLocaleString();
}

export function OAuth2TokenDisplay({ oauth2: o }: OAuth2TokenDisplayProps) {
  const [accessOpen, setAccessOpen] = useState(false);
  const [idOpen, setIdOpen] = useState(false);
  const [showRawPayload, setShowRawPayload] = useState(false);
  const [, setTick] = useState(0);

  // Re-render every 30s to update expiry countdown.
  useEffect(() => {
    if (!o.expiresIn || !o.tokenAcquiredAt) return;
    const interval = setInterval(() => setTick((t) => t + 1), 30_000);
    return () => clearInterval(interval);
  }, [o.expiresIn, o.tokenAcquiredAt]);

  if (!o.accessToken && !o.idToken) return null;

  const expired = isExpired(o.expiresIn, o.tokenAcquiredAt);

  return (
    <div className='rounded-md border border-border/50 bg-muted/20'>
      {/* Access Token panel */}
      {o.accessToken && (
        <Collapsible open={accessOpen} onOpenChange={setAccessOpen}>
          <CollapsibleTrigger className='flex w-full items-center justify-between px-3 py-2 text-sm hover:bg-muted/40 cursor-pointer'>
            <div className='flex items-center gap-2'>
              {accessOpen ? <ChevronDown className='h-3.5 w-3.5' /> : <ChevronRight className='h-3.5 w-3.5' />}
              <Key className='h-3.5 w-3.5 text-amber-500' />
              <span className='font-medium'>Access Token</span>
            </div>
            <span className={`text-xs ${expired ? 'text-destructive' : 'text-amber-600 dark:text-amber-400'}`}>
              {tokenExpiryDisplay(o.expiresIn, o.tokenAcquiredAt)}
            </span>
          </CollapsibleTrigger>
          <CollapsibleContent className='px-3 pb-3'>
            <div className='flex gap-1.5'>
              <Input className='flex-1 text-xs font-mono truncate' readOnly value={o.accessToken} />
              <Button
                variant='outline'
                size='sm'
                className='px-2 shrink-0'
                onClick={() => navigator.clipboard.writeText(o.accessToken)}
              >
                <Copy className='h-3 w-3' />
              </Button>
            </div>
          </CollapsibleContent>
        </Collapsible>
      )}

      {/* ID Token panel */}
      {o.idToken && (
        <Collapsible open={idOpen} onOpenChange={setIdOpen}>
          <CollapsibleTrigger className='flex w-full items-center gap-2 px-3 py-2 text-sm hover:bg-muted/40 cursor-pointer border-t border-border/30'>
            {idOpen ? <ChevronDown className='h-3.5 w-3.5' /> : <ChevronRight className='h-3.5 w-3.5' />}
            <Key className='h-3.5 w-3.5 text-blue-500' />
            <span className='font-medium'>ID Token</span>
          </CollapsibleTrigger>
          <CollapsibleContent className='px-3 pb-3 space-y-1.5'>
            {o.idTokenClaims ? (
              <>
                {o.idTokenClaims.subject && (
                  <div className='flex text-xs'><span className='w-20 text-muted-foreground'>Subject</span><span className='font-mono'>{o.idTokenClaims.subject}</span></div>
                )}
                {o.idTokenClaims.issuer && (
                  <div className='flex text-xs'><span className='w-20 text-muted-foreground'>Issuer</span><span className='font-mono truncate'>{o.idTokenClaims.issuer}</span></div>
                )}
                {o.idTokenClaims.audience && (
                  <div className='flex text-xs'><span className='w-20 text-muted-foreground'>Audience</span><span className='font-mono'>{o.idTokenClaims.audience}</span></div>
                )}
                {o.idTokenClaims.expiry && (
                  <div className='flex text-xs'><span className='w-20 text-muted-foreground'>Expires</span><span>{formatTimestamp(o.idTokenClaims.expiry)}</span></div>
                )}
                {o.idTokenClaims.issuedAt && (
                  <div className='flex text-xs'><span className='w-20 text-muted-foreground'>Issued At</span><span>{formatTimestamp(o.idTokenClaims.issuedAt)}</span></div>
                )}
                {o.idTokenClaims.algorithm && (
                  <div className='flex text-xs'><span className='w-20 text-muted-foreground'>Algorithm</span><span>{o.idTokenClaims.algorithm}</span></div>
                )}
                <Button variant='ghost' size='sm' className='text-xs mt-1' onClick={() => setShowRawPayload(!showRawPayload)}>
                  {showRawPayload ? 'Hide' : 'View'} Raw Payload
                </Button>
                {showRawPayload && (
                  <pre className='text-xs font-mono bg-muted p-2 rounded max-h-40 overflow-auto whitespace-pre-wrap'>
                    {o.idTokenClaims.rawPayload}
                  </pre>
                )}
              </>
            ) : (
              <div className='flex gap-1.5'>
                <Input className='flex-1 text-xs font-mono truncate' readOnly value={o.idToken} />
                <Button variant='outline' size='sm' className='px-2 shrink-0' onClick={() => navigator.clipboard.writeText(o.idToken)}>
                  <Copy className='h-3 w-3' />
                </Button>
              </div>
            )}
          </CollapsibleContent>
        </Collapsible>
      )}

      {/* Summary row */}
      {(o.tokenType || o.responseScope) && (
        <div className='flex gap-4 px-3 py-1.5 text-xs text-muted-foreground border-t border-border/30'>
          {o.tokenType && <span>Token Type: {o.tokenType}</span>}
          {o.responseScope && <span>Scope: {o.responseScope}</span>}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Create OAuth2SettingsSection.tsx**

```tsx
import { HelpCircle, Settings } from 'lucide-react';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { AuthState } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;

interface OAuth2SettingsSectionProps {
  oauth2: OAuth2State;
  patchOAuth2: (patch: Partial<OAuth2State>) => void;
}

export function OAuth2SettingsSection({ oauth2: o, patchOAuth2 }: OAuth2SettingsSectionProps) {
  return (
    <div className='space-y-2'>
      <div className='flex items-center gap-2 text-sm font-medium'>
        <Settings className='h-4 w-4 text-muted-foreground' />
        Settings
      </div>
      <div className='space-y-2 pl-1'>
        <div className='flex items-center gap-2'>
          <Checkbox
            id='oauth2-auto-fetch'
            checked={o.autoFetchToken}
            onCheckedChange={(checked) => patchOAuth2({ autoFetchToken: !!checked })}
          />
          <Label htmlFor='oauth2-auto-fetch' className='text-xs cursor-pointer'>
            Automatically fetch token if not found
          </Label>
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <HelpCircle className='h-3 w-3 text-muted-foreground' />
              </TooltipTrigger>
              <TooltipContent>
                <p className='text-xs max-w-52'>When enabled, a token will be fetched automatically before sending a request if no token is stored.</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>
        <div className='flex items-center gap-2'>
          <Checkbox
            id='oauth2-auto-refresh'
            checked={o.autoRefreshToken}
            onCheckedChange={(checked) => patchOAuth2({ autoRefreshToken: !!checked })}
          />
          <Label htmlFor='oauth2-auto-refresh' className='text-xs cursor-pointer'>
            Auto refresh token (with refresh URL)
          </Label>
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <HelpCircle className='h-3 w-3 text-muted-foreground' />
              </TooltipTrigger>
              <TooltipContent>
                <p className='text-xs max-w-52'>When enabled and the token is expired, it will be refreshed automatically using the refresh token before sending a request.</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Create OAuth2AdvancedSection.tsx**

```tsx
import { Sliders } from 'lucide-react';
import { SingleLineEditor } from '@/components/editor';
import { Label } from '@/components/ui/label';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { AuthState } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;

interface OAuth2AdvancedSectionProps {
  oauth2: OAuth2State;
  patchOAuth2: (patch: Partial<OAuth2State>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

export function OAuth2AdvancedSection({ oauth2: o, patchOAuth2, variableContext, onNavigateToSource }: OAuth2AdvancedSectionProps) {
  // Not shown for implicit grant.
  if (o.grantType === 'implicit') return null;

  return (
    <div className='space-y-2'>
      <div className='flex items-center gap-2 text-sm font-medium'>
        <Sliders className='h-4 w-4 text-amber-500' />
        Advanced Settings
      </div>
      <div className='pl-1'>
        <Label className='mb-1 block text-xs'>Refresh Token URL</Label>
        <SingleLineEditor
          className='text-sm font-mono'
          placeholder='Leave empty to use the Token URL'
          value={o.refreshTokenUrl}
          onChange={(newVal) => patchOAuth2({ refreshTokenUrl: newVal })}
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
        />
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Verify TypeScript**

```bash
npx tsc --noEmit 2>&1 | head -20
```

- [ ] **Step 6: Commit**

```bash
git add src/components/request/oauth2/
git commit -m "feat: OAuth2TokenDisplay, SettingsSection, AdvancedSection components"
```

---

### Task 2: OAuth2ConfigSection + OAuth2TokenSection

**Files:**
- Create: `src/components/request/oauth2/OAuth2ConfigSection.tsx`
- Create: `src/components/request/oauth2/OAuth2TokenSection.tsx`

- [ ] **Step 1: Create OAuth2ConfigSection.tsx**

This is the largest sub-component — handles all Configuration fields with conditional visibility per grant type. Read the spec Section 3 for the full field visibility matrix.

The component should include: Callback URL, Use system browser checkbox, Authorization URL, Access Token URL (labeled "Access Token URL" to match Bruno, not "Token URL"), Client ID, Client Secret, Scope, State, Add Credentials to dropdown, Use PKCE checkbox, Username/Password (password grant only).

Use `SingleLineEditor` for URL/credential fields (variable-aware), shadcn `Checkbox` for toggles, shadcn `Select` for dropdowns. Follow the existing AuthEditor patterns for field layout (Label + input stacked vertically, `space-y-3` between fields).

The component should be approximately 150-200 lines. Use the field visibility table from the spec to conditionally render each field.

- [ ] **Step 2: Create OAuth2TokenSection.tsx**

Simpler component — Token Source dropdown, Token ID input, Add token to dropdown, Header Prefix input. Approximately 80-100 lines.

```
🔑 Token
  Token Source:    [Access Token ▼]    (options: Access Token, ID Token)
  Token ID:        [Sage ID user     ]
  Add token to:    [Headers ▼]         (options: Headers, Query Params)
  Header Prefix:   [Bearer           ]
```

- [ ] **Step 3: Verify TypeScript**

```bash
npx tsc --noEmit 2>&1 | head -20
```

- [ ] **Step 4: Commit**

```bash
git add src/components/request/oauth2/
git commit -m "feat: OAuth2ConfigSection and OAuth2TokenSection components"
```

---

### Task 3: OAuth2AdditionalParams

**Files:**
- Create: `src/components/request/oauth2/OAuth2AdditionalParams.tsx`

- [ ] **Step 1: Create the tabbed key-value editor**

Uses shadcn `Tabs` with 3 tabs (Authorization, Token, Refresh). Each tab renders a table with: Key (SingleLineEditor), Value (SingleLineEditor), Send In (Select: queryparams/body), Enabled (Checkbox), Delete (Trash2 icon button). "+ Add Parameter" button at bottom.

Tab visibility per grant type (hidden, not disabled):
- Authorization: auth_code + implicit only
- Token: all except implicit
- Refresh: all grants

The component should handle adding, removing, toggling, and editing rows for each of the 3 param arrays. Approximately 150-200 lines.

- [ ] **Step 2: Verify TypeScript**

```bash
npx tsc --noEmit 2>&1 | head -20
```

- [ ] **Step 3: Commit**

```bash
git add src/components/request/oauth2/OAuth2AdditionalParams.tsx
git commit -m "feat: OAuth2AdditionalParams tabbed key-value editor"
```
