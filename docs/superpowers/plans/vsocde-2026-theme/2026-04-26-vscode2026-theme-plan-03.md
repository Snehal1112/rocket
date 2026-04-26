# VSCode 2026 Theme — Plan 03: Populate Playground Sections

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace every placeholder `SectionRenderer` in the playground with real interactive component showcases using shadcn/ui primitives — one section per component, four RocketAPI-specific pattern sections.

**Architecture:** Each section is a self-contained `.tsx` file in `apps/theme-playground/src/sections/`. `App.tsx` is updated to lazy-import them. Components are imported from `@/ui/*` (the path alias pointing to `frontend/src/components/ui/`).

**Tech Stack:** React 18, TypeScript, shadcn/ui (via `@/ui` alias), lucide-react, Tailwind CSS

**Spec:** `docs/superpowers/specs/2026-04-26-vscode2026-theme-design.md`
**Branch:** `feat/vscode-2026-theme` — **continue in the worktree from Plans 01 and 02**
**Prerequisite:** Plan 02 complete (playground scaffold running)

---

## Chunk 0: Enter the existing worktree

- [ ] **Step 1: Switch into the worktree**

```bash
cd .worktrees/vscode-2026-theme
git branch --show-current
# Expected: feat/vscode-2026-theme
```

Verify the playground scaffold is in place:

```bash
ls apps/theme-playground/src/
# Expected: App.tsx  ThemeToggle.tsx  globals.css  main.tsx  sections/
```

All tasks in this plan run from inside `.worktrees/vscode-2026-theme/`.

---

## Task 1: Primitive component sections — Button, Input, Checkbox, Badge

**Files:**
- Create: `apps/theme-playground/src/sections/ButtonSection.tsx`
- Create: `apps/theme-playground/src/sections/InputSection.tsx`
- Create: `apps/theme-playground/src/sections/CheckboxSection.tsx`
- Create: `apps/theme-playground/src/sections/BadgeSection.tsx`

> **Pattern for every section:** Each file exports a default named component (e.g. `ButtonSection`). Each section has a `<SectionShell title="..." desc="...">` wrapper (defined at the bottom of this task). Every interactive state is shown simultaneously — no hidden states.

- [ ] **Step 1: Create the shared `SectionShell` layout component**

Create `apps/theme-playground/src/sections/SectionShell.tsx`:

```tsx
import type { ReactNode } from 'react';

interface Props {
  title: string;
  desc?: string;
  children: ReactNode;
}

export function SectionShell({ title, desc, children }: Props) {
  return (
    <div className="max-w-3xl space-y-8">
      <div>
        <h2 className="text-lg font-semibold text-foreground">{title}</h2>
        {desc && <p className="mt-1 text-sm text-muted-foreground">{desc}</p>}
      </div>
      {children}
    </div>
  );
}

interface GroupProps {
  label: string;
  children: ReactNode;
}

export function ShowGroup({ label, children }: GroupProps) {
  return (
    <div>
      <p className="mb-3 text-[10px] font-semibold uppercase tracking-widest text-muted-foreground/60">
        {label}
      </p>
      <div className="flex flex-wrap items-center gap-3">{children}</div>
    </div>
  );
}
```

- [ ] **Step 2: Create `ButtonSection.tsx`**

Create `apps/theme-playground/src/sections/ButtonSection.tsx`:

```tsx
import { Send, Save, Plus, Trash2, X } from 'lucide-react';
import { Button } from '@/ui/button';
import { SectionShell, ShowGroup } from './SectionShell';

export function ButtonSection() {
  return (
    <SectionShell
      title="Button"
      desc="All shadcn Button variants. Primary maps to button.background (#0069CC light / #297AA0 dark)."
    >
      <ShowGroup label="Variants">
        <Button variant="default">Default</Button>
        <Button variant="secondary">Secondary</Button>
        <Button variant="outline">Outline</Button>
        <Button variant="ghost">Ghost</Button>
        <Button variant="destructive">Destructive</Button>
        <Button variant="link">Link</Button>
      </ShowGroup>

      <ShowGroup label="Sizes">
        <Button size="lg">Large</Button>
        <Button size="default">Default</Button>
        <Button size="sm">Small</Button>
        <Button size="icon"><Plus className="h-4 w-4" /></Button>
        <Button size="icon" variant="ghost" className="h-8 w-8"><X className="h-4 w-4" /></Button>
      </ShowGroup>

      <ShowGroup label="With icons">
        <Button><Send className="h-4 w-4" />Send request</Button>
        <Button variant="outline"><Save className="h-4 w-4" />Save</Button>
        <Button variant="destructive" size="sm"><Trash2 className="h-3.5 w-3.5" />Delete collection</Button>
      </ShowGroup>

      <ShowGroup label="Disabled">
        <Button disabled>Default</Button>
        <Button variant="outline" disabled>Outline</Button>
        <Button variant="secondary" disabled>Secondary</Button>
      </ShowGroup>

      <ShowGroup label="RocketAPI usage">
        <Button size="sm">Send</Button>
        <Button size="sm" variant="outline">Save</Button>
        <Button size="sm" variant="ghost">Add header</Button>
        <Button size="sm" variant="destructive">Delete</Button>
        <Button size="sm" variant="secondary">Cancel</Button>
        <Button size="sm" variant="outline">Import from Bruno</Button>
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 3: Create `InputSection.tsx`**

Create `apps/theme-playground/src/sections/InputSection.tsx`:

```tsx
import { Input } from '@/ui/input';
import { Label } from '@/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/ui/select';
import { Textarea } from '@/ui/textarea';
import { SectionShell, ShowGroup } from './SectionShell';

export function InputSection() {
  return (
    <SectionShell
      title="Input / Label / Select / Textarea"
      desc="input.background → --card. input.border → --input. focusBorder → --ring."
    >
      <ShowGroup label="Basic inputs">
        <div className="flex flex-col gap-1.5 w-52">
          <Label>Request name</Label>
          <Input placeholder="Get user by ID" />
        </div>
        <div className="flex flex-col gap-1.5 w-52">
          <Label>Client ID</Label>
          <Input placeholder="client_xxxx" className="font-mono text-xs" />
        </div>
        <div className="flex flex-col gap-1.5 w-40">
          <Label>Disabled</Label>
          <Input placeholder="Read only" disabled />
        </div>
      </ShowGroup>

      <ShowGroup label="Input group — URL bar">
        <div className="flex items-stretch rounded-md border border-input overflow-hidden w-[420px] focus-within:ring-1 focus-within:ring-ring focus-within:border-ring transition-colors">
          <Select defaultValue="GET">
            <SelectTrigger className="h-9 w-24 rounded-none border-0 border-r bg-muted text-xs focus:ring-0">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {['GET','POST','PUT','PATCH','DELETE','HEAD'].map((m) => (
                <SelectItem key={m} value={m} className="font-mono text-xs">{m}</SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Input
            defaultValue="{{baseUrl}}/users/{{userId}}"
            className="rounded-none border-0 focus-visible:ring-0 font-mono text-xs flex-1"
          />
          <div className="flex items-center gap-1 px-2 border-l bg-muted">
            <button className="text-xs text-muted-foreground px-2 py-1 rounded hover:bg-background transition-colors">Save</button>
            <button className="text-xs bg-primary text-primary-foreground px-2 py-1 rounded hover:opacity-90 transition-opacity">Send</button>
          </div>
        </div>
      </ShowGroup>

      <ShowGroup label="Validation states">
        <div className="flex flex-col gap-1.5 w-52">
          <Label>Valid</Label>
          <Input defaultValue="my-collection" className="border-green-500 focus-visible:ring-green-500/20" />
        </div>
        <div className="flex flex-col gap-1.5 w-52">
          <Label>Error</Label>
          <Input defaultValue="duplicate name" className="border-destructive focus-visible:ring-destructive/20" />
          <p className="text-[11px] text-destructive">Name already exists in this workspace</p>
        </div>
      </ShowGroup>

      <ShowGroup label="Select">
        <Select defaultValue="staging">
          <SelectTrigger className="w-44">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="staging">Staging</SelectItem>
            <SelectItem value="production">Production</SelectItem>
            <SelectItem value="local">Local</SelectItem>
          </SelectContent>
        </Select>
        <Select disabled defaultValue="disabled">
          <SelectTrigger className="w-36">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="disabled">Disabled</SelectItem>
          </SelectContent>
        </Select>
      </ShowGroup>

      <ShowGroup label="Textarea">
        <div className="flex flex-col gap-1.5 w-80">
          <Label>Collection description</Label>
          <Textarea placeholder="Describe what this collection tests..." className="resize-none" rows={3} />
        </div>
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 4: Create `CheckboxSection.tsx`**

Create `apps/theme-playground/src/sections/CheckboxSection.tsx`:

```tsx
import { useState } from 'react';
import { Checkbox } from '@/ui/checkbox';
import { Label } from '@/ui/label';
import { Switch } from '@/ui/switch';
import { RadioGroup, RadioGroupItem } from '@/ui/radio-group';
import { SectionShell, ShowGroup } from './SectionShell';

export function CheckboxSection() {
  const [checked, setChecked] = useState(true);
  const [switchVal, setSwitchVal] = useState(true);

  return (
    <SectionShell
      title="Checkbox / Switch / Radio"
      desc="checkbox.background → --secondary. checkbox.border → --border. Checked fill → --primary."
    >
      <ShowGroup label="Checkbox">
        <div className="flex items-center gap-2">
          <Checkbox id="cb1" checked={checked} onCheckedChange={(v) => setChecked(!!v)} />
          <Label htmlFor="cb1">Enable header</Label>
        </div>
        <div className="flex items-center gap-2">
          <Checkbox id="cb2" />
          <Label htmlFor="cb2">Enable query param</Label>
        </div>
        <div className="flex items-center gap-2 opacity-50">
          <Checkbox id="cb3" checked disabled />
          <Label htmlFor="cb3">Disabled checked</Label>
        </div>
      </ShowGroup>

      <ShowGroup label="Switch">
        <div className="flex flex-col gap-3">
          {[
            ['Follow redirects', switchVal, setSwitchVal],
            ['Verify SSL certificate', false, () => {}],
            ['Send cookies automatically', true, () => {}],
          ].map(([label, val, set], i) => (
            <div key={i} className="flex items-center gap-3">
              <Switch
                checked={!!val}
                onCheckedChange={typeof set === 'function' ? set as (v: boolean) => void : undefined}
              />
              <Label>{label as string}</Label>
            </div>
          ))}
        </div>
      </ShowGroup>

      <ShowGroup label="Radio group — import target">
        <RadioGroup defaultValue="new-workspace" className="flex flex-col gap-2">
          <div className="flex items-center gap-2">
            <RadioGroupItem value="new-workspace" id="r1" />
            <Label htmlFor="r1">Create new workspace</Label>
          </div>
          <div className="flex items-center gap-2">
            <RadioGroupItem value="current-workspace" id="r2" />
            <Label htmlFor="r2">Add to current workspace</Label>
          </div>
        </RadioGroup>
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 5: Create `BadgeSection.tsx`**

Create `apps/theme-playground/src/sections/BadgeSection.tsx`:

```tsx
import { Badge } from '@/ui/badge';
import { SectionShell, ShowGroup } from './SectionShell';
import { cn } from '@/lib/utils';

function MethodBadge({ method }: { method: string }) {
  const colors: Record<string, string> = {
    GET:    'text-primary bg-primary/10',
    POST:   'text-green-600 bg-green-500/10 dark:text-green-400 dark:bg-green-500/15',
    PUT:    'text-amber-600 bg-amber-500/10 dark:text-amber-400 dark:bg-amber-500/15',
    PATCH:  'text-amber-600 bg-amber-500/10 dark:text-amber-400 dark:bg-amber-500/15',
    DELETE: 'text-destructive bg-destructive/10',
    HEAD:   'text-muted-foreground bg-muted',
  };
  return (
    <span className={cn('inline-block font-mono font-bold text-[10px] px-1.5 py-0.5 rounded min-w-[42px] text-center', colors[method])}>
      {method}
    </span>
  );
}

function StatusBadge({ code }: { code: number }) {
  const color =
    code < 300 ? 'bg-green-500/10 text-green-600 dark:text-green-400' :
    code < 400 ? 'bg-blue-500/10 text-blue-600 dark:text-blue-400' :
    code < 500 ? 'bg-amber-500/10 text-amber-600 dark:text-amber-400' :
                 'bg-destructive/10 text-destructive';
  return (
    <span className={cn('inline-block font-mono text-[10px] font-semibold px-1.5 py-0.5 rounded', color)}>
      {code}
    </span>
  );
}

export function BadgeSection() {
  return (
    <SectionShell
      title="Badge"
      desc="Uses badge.background for primary. Semantic colors for status variants."
    >
      <ShowGroup label="Variants">
        <Badge>Default</Badge>
        <Badge variant="secondary">Secondary</Badge>
        <Badge variant="outline">Outline</Badge>
        <Badge variant="destructive">Destructive</Badge>
      </ShowGroup>

      <ShowGroup label="HTTP methods">
        {['GET','POST','PUT','PATCH','DELETE','HEAD'].map((m) => (
          <MethodBadge key={m} method={m} />
        ))}
      </ShowGroup>

      <ShowGroup label="Response status codes">
        {[200, 201, 204, 301, 400, 401, 403, 404, 422, 500, 502].map((c) => (
          <StatusBadge key={c} code={c} />
        ))}
      </ShowGroup>

      <ShowGroup label="Git decoration">
        {[
          { label: 'A', cls: 'text-green-600 bg-green-500/10 dark:text-green-400' },
          { label: 'M', cls: 'text-amber-600 bg-amber-500/10 dark:text-amber-400' },
          { label: 'D', cls: 'text-destructive bg-destructive/10' },
          { label: 'I', cls: 'text-muted-foreground bg-muted' },
          { label: 'C', cls: 'text-destructive bg-destructive/10' },
        ].map(({ label, cls }) => (
          <span key={label} className={cn('font-mono font-bold text-[10px] px-1.5 py-0.5 rounded', cls)}>{label}</span>
        ))}
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 6: Wire all four sections into `App.tsx`**

In `apps/theme-playground/src/App.tsx`, replace the `SectionRenderer` function with dynamic imports:

```tsx
// Add at the top of the file (after existing imports):
import { lazy, Suspense } from 'react';

const sectionMap: Record<string, React.LazyExoticComponent<() => JSX.Element>> = {
  buttons:    lazy(() => import('./sections/ButtonSection').then(m => ({ default: m.ButtonSection }))),
  inputs:     lazy(() => import('./sections/InputSection').then(m => ({ default: m.InputSection }))),
  checkbox:   lazy(() => import('./sections/CheckboxSection').then(m => ({ default: m.CheckboxSection }))),
  badges:     lazy(() => import('./sections/BadgeSection').then(m => ({ default: m.BadgeSection }))),
};

function SectionRenderer({ id }: { id: SectionId }) {
  const Component = sectionMap[id];
  if (!Component) {
    return (
      <div className="rounded-lg border border-dashed border-border p-8 text-center text-muted-foreground text-sm">
        <p className="text-base font-medium text-foreground mb-1">{id}</p>
        <p>Coming soon.</p>
      </div>
    );
  }
  return (
    <Suspense fallback={<div className="text-muted-foreground text-sm">Loading...</div>}>
      <Component />
    </Suspense>
  );
}
```

- [ ] **Step 7: Verify all four sections render**

```bash
cd apps/theme-playground
yarn dev
```

Click through: Button, Input/Label, Checkbox/Switch, Badge. Each must render without errors in both light and dark mode.

- [ ] **Step 8: Commit**

```bash
cd ../..
git add apps/theme-playground/src/sections/
git add apps/theme-playground/src/App.tsx
git commit -m "feat(playground): add Button, Input, Checkbox, Badge sections

All use @/ui path alias pointing at frontend/src/components/ui.
Lazy-loaded via React.lazy — each section is an independent chunk."
```

---

## Task 2: Navigation and overlay sections — Tabs, Table, Card, Alert, Dropdown, Collapsible, Progress, Separator

**Files:**
- Create: `apps/theme-playground/src/sections/TabsSection.tsx`
- Create: `apps/theme-playground/src/sections/TableSection.tsx`
- Create: `apps/theme-playground/src/sections/CardSection.tsx`
- Create: `apps/theme-playground/src/sections/AlertSection.tsx`
- Create: `apps/theme-playground/src/sections/DropdownSection.tsx`
- Create: `apps/theme-playground/src/sections/CollapsibleSection.tsx`
- Create: `apps/theme-playground/src/sections/ProgressSection.tsx`
- Create: `apps/theme-playground/src/sections/SeparatorSection.tsx`

- [ ] **Step 1: Create `TabsSection.tsx`**

```tsx
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/ui/tabs';
import { SectionShell, ShowGroup } from './SectionShell';

export function TabsSection() {
  return (
    <SectionShell
      title="Tabs"
      desc="Two patterns: underline (main request nav) and filled pill (response viewer sub-nav)."
    >
      <ShowGroup label="Underline style — request panel">
        <Tabs defaultValue="params" className="w-full">
          <TabsList className="w-full justify-start rounded-none border-b bg-transparent h-9 px-0 gap-0">
            {['params','headers','body','auth','scripts','contract'].map((t) => (
              <TabsTrigger
                key={t}
                value={t}
                className="rounded-none border-b-2 border-transparent data-[state=active]:border-primary data-[state=active]:bg-transparent data-[state=active]:shadow-none capitalize h-9"
              >
                {t}
              </TabsTrigger>
            ))}
          </TabsList>
          <TabsContent value="params" className="mt-4 text-sm text-muted-foreground">Params panel content</TabsContent>
        </Tabs>
      </ShowGroup>

      <ShowGroup label="Filled pill style — response viewer">
        <Tabs defaultValue="pretty">
          <TabsList>
            <TabsTrigger value="pretty">Pretty</TabsTrigger>
            <TabsTrigger value="raw">Raw</TabsTrigger>
            <TabsTrigger value="preview">Preview</TabsTrigger>
            <TabsTrigger value="headers">Headers (4)</TabsTrigger>
          </TabsList>
        </Tabs>
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 2: Create `TableSection.tsx`**

```tsx
import { useState } from 'react';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/ui/table';
import { Checkbox } from '@/ui/checkbox';
import { Input } from '@/ui/input';
import { Button } from '@/ui/button';
import { X, Plus } from 'lucide-react';
import { SectionShell } from './SectionShell';

interface Row { id: string; enabled: boolean; key: string; value: string; }

export function TableSection() {
  const [rows, setRows] = useState<Row[]>([
    { id: '1', enabled: true,  key: 'Content-Type',  value: 'application/json' },
    { id: '2', enabled: false, key: 'Authorization',  value: 'Bearer {{token}}' },
    { id: '3', enabled: true,  key: 'X-Request-ID',   value: '{{$randomUUID}}' },
  ]);

  const update = (id: string, patch: Partial<Row>) =>
    setRows((rs) => rs.map((r) => (r.id === id ? { ...r, ...patch } : r)));
  const remove = (id: string) => setRows((rs) => rs.filter((r) => r.id !== id));
  const add = () =>
    setRows((rs) => [...rs, { id: String(Date.now()), enabled: true, key: '', value: '' }]);

  return (
    <SectionShell
      title="Table"
      desc="Used in KeyValueEditor, EnvironmentDialog, CollectionVariablesEditor. Row hover uses list.hoverBackground."
    >
      <div className="rounded-lg border border-border overflow-hidden max-w-2xl">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-8" />
              <TableHead>Key</TableHead>
              <TableHead>Value</TableHead>
              <TableHead className="w-8" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((row) => (
              <TableRow key={row.id}>
                <TableCell>
                  <Checkbox
                    checked={row.enabled}
                    onCheckedChange={(v) => update(row.id, { enabled: !!v })}
                  />
                </TableCell>
                <TableCell>
                  <Input
                    value={row.key}
                    onChange={(e) => update(row.id, { key: e.target.value })}
                    placeholder="Key"
                    className="h-7 text-xs font-mono border-0 bg-transparent focus-visible:ring-1 p-0"
                  />
                </TableCell>
                <TableCell>
                  <Input
                    value={row.value}
                    onChange={(e) => update(row.id, { value: e.target.value })}
                    placeholder="Value"
                    className="h-7 text-xs font-mono border-0 bg-transparent focus-visible:ring-1 p-0 text-primary"
                  />
                </TableCell>
                <TableCell>
                  <Button variant="ghost" size="icon" className="h-6 w-6 text-muted-foreground" onClick={() => remove(row.id)}>
                    <X className="h-3 w-3" />
                  </Button>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
      <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={add}>
        <Plus className="h-3.5 w-3.5" /> Add header
      </Button>
    </SectionShell>
  );
}
```

- [ ] **Step 3: Create `CardSection.tsx`**

```tsx
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from '@/ui/card';
import { Button } from '@/ui/button';
import { Input } from '@/ui/input';
import { Label } from '@/ui/label';
import { SectionShell, ShowGroup } from './SectionShell';

export function CardSection() {
  return (
    <SectionShell
      title="Card / Dialog"
      desc="editorWidget.background → --popover used as dialog bg. widget.border → --border."
    >
      <ShowGroup label="Info card">
        <Card className="w-72">
          <CardHeader>
            <CardTitle className="text-sm">Environment: Staging</CardTitle>
            <CardDescription>12 variables active, 2 override collection defaults.</CardDescription>
          </CardHeader>
        </Card>
      </ShowGroup>

      <ShowGroup label="Confirm dialog (AlertDialog pattern)">
        <Card className="w-80">
          <CardHeader>
            <CardTitle className="text-sm">Delete collection?</CardTitle>
            <CardDescription>
              This will permanently delete <strong>Users API</strong> and all 14 requests inside it.
            </CardDescription>
          </CardHeader>
          <CardFooter className="flex justify-end gap-2">
            <Button variant="outline" size="sm">Cancel</Button>
            <Button variant="destructive" size="sm">Delete</Button>
          </CardFooter>
        </Card>
      </ShowGroup>

      <ShowGroup label="Create dialog">
        <Card className="w-72">
          <CardHeader>
            <CardTitle className="text-sm">New workspace</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex flex-col gap-1.5">
              <Label>Name</Label>
              <Input defaultValue="My Project" className="h-8" />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label>Folder</Label>
              <Input defaultValue="/Users/snehal/projects" className="h-8 font-mono text-xs" />
            </div>
          </CardContent>
          <CardFooter className="flex justify-end gap-2">
            <Button variant="outline" size="sm">Cancel</Button>
            <Button size="sm">Create workspace</Button>
          </CardFooter>
        </Card>
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 4: Create `AlertSection.tsx`**

```tsx
import { Info, AlertTriangle, XCircle, CheckCircle2 } from 'lucide-react';
import { SectionShell, ShowGroup } from './SectionShell';

type Variant = 'info' | 'warning' | 'error' | 'success';

const configs = {
  info:    { Icon: Info,           bg: 'bg-primary/8',       border: 'border-primary/30',      icon: 'text-primary' },
  warning: { Icon: AlertTriangle,  bg: 'bg-amber-500/8',     border: 'border-amber-500/30',     icon: 'text-amber-500' },
  error:   { Icon: XCircle,        bg: 'bg-destructive/8',   border: 'border-destructive/30',   icon: 'text-destructive' },
  success: { Icon: CheckCircle2,   bg: 'bg-green-500/8',     border: 'border-green-500/30',     icon: 'text-green-600 dark:text-green-400' },
};

function Alert({ variant, title, body }: { variant: Variant; title: string; body: string }) {
  const { Icon, bg, border, icon } = configs[variant];
  return (
    <div className={`flex gap-3 rounded-lg border p-3 ${bg} ${border} max-w-md`}>
      <Icon className={`h-4 w-4 mt-0.5 shrink-0 ${icon}`} />
      <div>
        <p className="text-sm font-medium text-foreground">{title}</p>
        <p className="mt-0.5 text-xs text-muted-foreground">{body}</p>
      </div>
    </div>
  );
}

export function AlertSection() {
  return (
    <SectionShell title="Alert" desc="Semantic colors from inputValidation.* and notifications* tokens.">
      <ShowGroup label="Variants">
        <div className="flex flex-col gap-3">
          <Alert variant="info"    title="OAuth2 token refreshed"  body="New access token valid for 3600 seconds." />
          <Alert variant="warning" title="Variable unresolved"     body="{{token}} not found in any scope." />
          <Alert variant="error"   title="Request failed"          body="Connection refused at localhost:3000." />
          <Alert variant="success" title="Import successful"       body="42 requests imported across 5 collections." />
        </div>
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 5: Create `CollapsibleSection.tsx`**

```tsx
import { useState } from 'react';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/ui/collapsible';
import { ChevronDown } from 'lucide-react';
import { Input } from '@/ui/input';
import { Label } from '@/ui/label';
import { Switch } from '@/ui/switch';
import { SectionShell, ShowGroup } from './SectionShell';
import { cn } from '@/lib/utils';

export function CollapsibleSection() {
  const [open1, setOpen1] = useState(false);
  const [open2, setOpen2] = useState(true);

  return (
    <SectionShell
      title="Collapsible"
      desc="Used in AuthEditor OAuth2 advanced options, git stash details, import skipped items."
    >
      <ShowGroup label="Examples">
        <div className="flex flex-col gap-3 w-96">
          <Collapsible open={open1} onOpenChange={setOpen1} className="rounded-lg border border-border">
            <CollapsibleTrigger className="flex w-full items-center justify-between px-4 py-2.5 text-sm font-medium hover:bg-muted/50 transition-colors">
              Advanced OAuth2 options
              <ChevronDown className={cn('h-4 w-4 text-muted-foreground transition-transform', open1 && 'rotate-180')} />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="border-t border-border px-4 py-3 space-y-3">
                <div className="flex flex-col gap-1.5">
                  <Label className="text-xs">Scope</Label>
                  <Input placeholder="openid profile email" className="h-7 text-xs" />
                </div>
                <div className="flex items-center gap-3">
                  <Switch id="pkce" />
                  <Label htmlFor="pkce" className="text-xs">Use PKCE</Label>
                </div>
              </div>
            </CollapsibleContent>
          </Collapsible>

          <Collapsible open={open2} onOpenChange={setOpen2} className="rounded-lg border border-border">
            <CollapsibleTrigger className="flex w-full items-center justify-between px-4 py-2.5 text-sm font-medium hover:bg-muted/50 transition-colors">
              Skipped items (3)
              <ChevronDown className={cn('h-4 w-4 text-muted-foreground transition-transform', open2 && 'rotate-180')} />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="border-t border-border px-4 py-3 space-y-1.5">
                {['auth/oauth2-flow.bru — unsupported auth type', 'grpc/stream.bru — unsupported type', 'ws/chat.bru — WebSocket not supported'].map((msg) => (
                  <p key={msg} className="font-mono text-[11px] text-muted-foreground">{msg}</p>
                ))}
              </div>
            </CollapsibleContent>
          </Collapsible>
        </div>
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 6: Create `ProgressSection.tsx`**

```tsx
import { Progress } from '@/ui/progress';
import { SectionShell, ShowGroup } from './SectionShell';

export function ProgressSection() {
  return (
    <SectionShell
      title="Progress"
      desc="progressBar.background → --primary in light. Used in load test runner and Bruno import."
    >
      <ShowGroup label="Examples">
        <div className="flex flex-col gap-4 w-80">
          <div>
            <div className="flex justify-between text-xs text-muted-foreground mb-1.5">
              <span>Load test — 200 virtual users</span><span>67%</span>
            </div>
            <Progress value={67} className="h-1.5" />
            <p className="mt-1 text-[11px] text-muted-foreground">134 / 200 req · 142 ms avg</p>
          </div>
          <div>
            <div className="flex justify-between text-xs text-muted-foreground mb-1.5">
              <span>Bruno import</span><span>100%</span>
            </div>
            <Progress value={100} className="h-1.5 [&>div]:bg-green-500" />
            <p className="mt-1 text-[11px] text-green-600 dark:text-green-400">42 requests imported</p>
          </div>
          <div>
            <div className="flex justify-between text-xs text-muted-foreground mb-1.5">
              <span>Contract validation</span><span>3 failures</span>
            </div>
            <Progress value={30} className="h-1.5 [&>div]:bg-destructive" />
            <p className="mt-1 text-[11px] text-destructive">3 / 10 assertions failed</p>
          </div>
        </div>
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 7: Create `DropdownSection.tsx` and `SeparatorSection.tsx`**

Create `apps/theme-playground/src/sections/DropdownSection.tsx`:

```tsx
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuShortcut, DropdownMenuTrigger } from '@/ui/dropdown-menu';
import { Button } from '@/ui/button';
import { MoreHorizontal, ChevronDown } from 'lucide-react';
import { SectionShell, ShowGroup } from './SectionShell';

export function DropdownSection() {
  return (
    <SectionShell
      title="DropdownMenu"
      desc="menu.background → --popover. menu.selectionBackground → --accent. separatorBackground → --border."
    >
      <ShowGroup label="Request context menu">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm"><MoreHorizontal className="h-4 w-4" /></Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent>
            <DropdownMenuItem>Open in new tab<DropdownMenuShortcut>⌘T</DropdownMenuShortcut></DropdownMenuItem>
            <DropdownMenuItem>Rename request</DropdownMenuItem>
            <DropdownMenuItem>Duplicate<DropdownMenuShortcut>⌘D</DropdownMenuShortcut></DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem>Copy as cURL</DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem className="text-destructive">Delete request</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </ShowGroup>

      <ShowGroup label="Workspace switcher">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm" className="gap-1">Personal workspace<ChevronDown className="h-3.5 w-3.5" /></Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent className="w-52">
            <DropdownMenuItem className="text-primary font-medium">Personal workspace</DropdownMenuItem>
            <DropdownMenuItem>Sage · API Team</DropdownMenuItem>
            <DropdownMenuItem>Sage · Platform</DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem className="text-primary">+ New workspace</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </ShowGroup>
    </SectionShell>
  );
}
```

Create `apps/theme-playground/src/sections/SeparatorSection.tsx`:

```tsx
import { Separator } from '@/ui/separator';
import { SectionShell, ShowGroup } from './SectionShell';

export function SeparatorSection() {
  return (
    <SectionShell title="Separator" desc="menu.separatorBackground → --border. Used in dropdowns and form layouts.">
      <ShowGroup label="Horizontal">
        <div className="w-64">
          <p className="text-sm py-2">Above</p>
          <Separator />
          <p className="text-sm text-muted-foreground py-2">Below</p>
        </div>
      </ShowGroup>
      <ShowGroup label="Vertical">
        <div className="flex items-center gap-4 h-8">
          <span className="text-sm">Left</span>
          <Separator orientation="vertical" />
          <span className="text-sm text-muted-foreground">Right</span>
        </div>
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 8: Wire all 8 sections into `App.tsx`**

In `apps/theme-playground/src/App.tsx`, add to `sectionMap`:

```tsx
tabs:         lazy(() => import('./sections/TabsSection').then(m => ({ default: m.TabsSection }))),
table:        lazy(() => import('./sections/TableSection').then(m => ({ default: m.TableSection }))),
cards:        lazy(() => import('./sections/CardSection').then(m => ({ default: m.CardSection }))),
alerts:       lazy(() => import('./sections/AlertSection').then(m => ({ default: m.AlertSection }))),
dropdown:     lazy(() => import('./sections/DropdownSection').then(m => ({ default: m.DropdownSection }))),
collapsible:  lazy(() => import('./sections/CollapsibleSection').then(m => ({ default: m.CollapsibleSection }))),
progress:     lazy(() => import('./sections/ProgressSection').then(m => ({ default: m.ProgressSection }))),
separator:    lazy(() => import('./sections/SeparatorSection').then(m => ({ default: m.SeparatorSection }))),
```

- [ ] **Step 9: Verify all sections render — no errors in either theme**

```bash
cd apps/theme-playground
yarn dev
```

Click through every section in both light and dark mode. No console errors.

- [ ] **Step 10: Commit**

```bash
cd ../..
git add apps/theme-playground/src/sections/
git add apps/theme-playground/src/App.tsx
git commit -m "feat(playground): add Tabs, Table, Card, Alert, Dropdown, Collapsible, Progress, Separator sections"
```

---

## Task 3: RocketAPI pattern sections

**Files:**
- Create: `apps/theme-playground/src/sections/RequestBarSection.tsx`
- Create: `apps/theme-playground/src/sections/KeyValueSection.tsx`
- Create: `apps/theme-playground/src/sections/GitStatusSection.tsx`
- Create: `apps/theme-playground/src/sections/EnvSwitcherSection.tsx`

- [ ] **Step 1: Create `RequestBarSection.tsx`**

```tsx
import { useState } from 'react';
import { Input } from '@/ui/input';
import { Button } from '@/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/ui/select';
import { Badge } from '@/ui/badge';
import { SectionShell, ShowGroup } from './SectionShell';

export function RequestBarSection() {
  const [url, setUrl] = useState('{{baseUrl}}/users/{{userId}}');
  const [dirty, setDirty] = useState(true);

  return (
    <SectionShell
      title="Request Bar"
      desc="The primary interaction surface. URL bar + method selector + send/save + unsaved indicator."
    >
      <ShowGroup label="Full request bar">
        <div className="w-full max-w-2xl rounded-xl border border-border bg-card p-3 space-y-2.5">
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">GET /users/&#123;&#123;userId&#125;&#125;</span>
            {dirty && <Badge variant="outline" className="text-[10px] py-0 h-4 text-amber-600 border-amber-300 dark:text-amber-400 dark:border-amber-700">unsaved</Badge>}
            <div className="ml-auto flex items-center gap-2">
              <span className="text-[11px] text-muted-foreground">staging</span>
              <div className="flex items-center gap-1.5 rounded-full bg-primary/10 border border-primary/20 px-2.5 py-0.5">
                <span className="h-1.5 w-1.5 rounded-full bg-primary" />
                <span className="text-[11px] text-primary font-medium">staging</span>
              </div>
            </div>
          </div>
          <div className="flex items-stretch rounded-md border border-input overflow-hidden focus-within:ring-1 focus-within:ring-ring focus-within:border-ring transition-colors">
            <Select defaultValue="GET">
              <SelectTrigger className="h-9 w-24 rounded-none border-0 border-r bg-muted text-xs focus:ring-0 shrink-0">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {['GET','POST','PUT','PATCH','DELETE','HEAD'].map((m) => (
                  <SelectItem key={m} value={m} className="font-mono text-xs">{m}</SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Input
              value={url}
              onChange={(e) => { setUrl(e.target.value); setDirty(true); }}
              className="rounded-none border-0 focus-visible:ring-0 font-mono text-xs flex-1"
            />
            <div className="flex items-center gap-1 px-2 border-l bg-muted shrink-0">
              <Button variant="outline" size="sm" className="h-7 text-xs" onClick={() => setDirty(false)}>Save</Button>
              <Button size="sm" className="h-7 text-xs">Send</Button>
            </div>
          </div>
        </div>
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 2: Create `KeyValueSection.tsx`**

```tsx
import { useState } from 'react';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/ui/table';
import { Checkbox } from '@/ui/checkbox';
import { Input } from '@/ui/input';
import { Button } from '@/ui/button';
import { X, Plus } from 'lucide-react';
import { SectionShell, ShowGroup } from './SectionShell';

interface Row { id: string; enabled: boolean; key: string; value: string; }

export function KeyValueSection() {
  const [rows, setRows] = useState<Row[]>([
    { id: '1', enabled: true,  key: 'userId',  value: '{{currentUser.id}}' },
    { id: '2', enabled: true,  key: 'format',   value: 'json' },
  ]);

  const update = (id: string, patch: Partial<Row>) =>
    setRows((rs) => rs.map((r) => (r.id === id ? { ...r, ...patch } : r)));
  const remove = (id: string) => setRows((rs) => rs.filter((r) => r.id !== id));
  const add = () =>
    setRows((rs) => [...rs, { id: String(Date.now()), enabled: true, key: '', value: '' }]);

  return (
    <SectionShell
      title="Key-Value Editor"
      desc="Used for Params, Headers, Form-data, Collection Variables. shadcn Checkbox + Input + Button per row."
    >
      <ShowGroup label="Query params">
        <div className="w-full max-w-xl">
          <div className="rounded-lg border border-border overflow-hidden">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-8" />
                  <TableHead>Key</TableHead>
                  <TableHead>Value</TableHead>
                  <TableHead className="w-8" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((row) => (
                  <TableRow key={row.id}>
                    <TableCell>
                      <Checkbox checked={row.enabled} onCheckedChange={(v) => update(row.id, { enabled: !!v })} />
                    </TableCell>
                    <TableCell>
                      <Input value={row.key} onChange={(e) => update(row.id, { key: e.target.value })}
                        className="h-7 text-xs font-mono border-0 bg-transparent focus-visible:ring-1 p-0" placeholder="Key" />
                    </TableCell>
                    <TableCell>
                      <Input value={row.value} onChange={(e) => update(row.id, { value: e.target.value })}
                        className="h-7 text-xs font-mono border-0 bg-transparent focus-visible:ring-1 p-0 text-primary" placeholder="Value" />
                    </TableCell>
                    <TableCell>
                      <Button variant="ghost" size="icon" className="h-6 w-6 text-muted-foreground" onClick={() => remove(row.id)}>
                        <X className="h-3 w-3" />
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
          <Button variant="ghost" size="sm" className="mt-2 text-muted-foreground text-xs" onClick={add}>
            <Plus className="h-3.5 w-3.5" /> Add parameter
          </Button>
        </div>
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 3: Create `GitStatusSection.tsx`**

```tsx
import { Badge } from '@/ui/badge';
import { Button } from '@/ui/button';
import { Separator } from '@/ui/separator';
import { SectionShell, ShowGroup } from './SectionShell';
import { cn } from '@/lib/utils';

const files = [
  { status: 'A', path: 'users/create-user.yml',       cls: 'text-green-600 bg-green-500/10 dark:text-green-400' },
  { status: 'M', path: 'users/get-user.yml',           cls: 'text-amber-600 bg-amber-500/10 dark:text-amber-400' },
  { status: 'M', path: 'environments/staging.yml',     cls: 'text-amber-600 bg-amber-500/10 dark:text-amber-400' },
  { status: 'D', path: 'auth/legacy-basic.yml',        cls: 'text-destructive bg-destructive/10' },
  { status: 'I', path: '.env.local',                   cls: 'text-muted-foreground bg-muted' },
];

export function GitStatusSection() {
  return (
    <SectionShell
      title="Git Status"
      desc="gitDecoration.* colors from the 2026 theme — Added, Modified, Deleted, Ignored."
    >
      <ShowGroup label="Changes panel">
        <div className="w-80 rounded-xl border border-border bg-card p-4">
          <div className="flex items-center gap-2 mb-3">
            <span className="text-sm font-semibold text-foreground">Changes</span>
            <Badge className="h-4 text-[10px] px-1.5">5</Badge>
          </div>
          <div className="space-y-0.5">
            {files.map((f) => (
              <div key={f.path} className="flex items-center gap-2 px-2 py-1.5 rounded-md hover:bg-muted/50 transition-colors cursor-pointer">
                <span className={cn('font-mono font-bold text-[10px] px-1.5 py-0.5 rounded shrink-0', f.cls)}>{f.status}</span>
                <span className="font-mono text-[11px] text-muted-foreground truncate">{f.path}</span>
              </div>
            ))}
          </div>
          <Separator className="my-3" />
          <div className="flex justify-end gap-2">
            <Button variant="outline" size="sm" className="h-7 text-xs">Stage all</Button>
            <Button size="sm" className="h-7 text-xs">Commit</Button>
          </div>
        </div>
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 4: Create `EnvSwitcherSection.tsx`**

```tsx
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '@/ui/dropdown-menu';
import { Button } from '@/ui/button';
import { ChevronDown } from 'lucide-react';
import { SectionShell, ShowGroup } from './SectionShell';

const envs = ['staging', 'production', 'local'];

export function EnvSwitcherSection() {
  return (
    <SectionShell
      title="Environment Switcher"
      desc="Env badge uses list.activeSelectionBackground. Active dot uses --primary."
    >
      <ShowGroup label="Dropdown trigger">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm" className="gap-1.5 h-7 text-xs">
              <span className="h-1.5 w-1.5 rounded-full bg-primary" />
              staging
              <ChevronDown className="h-3 w-3 text-muted-foreground" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent className="w-44">
            <DropdownMenuItem className="text-primary font-medium gap-2">
              <span className="h-1.5 w-1.5 rounded-full bg-primary shrink-0" /> staging
            </DropdownMenuItem>
            {envs.slice(1).map((e) => (
              <DropdownMenuItem key={e} className="gap-2">
                <span className="h-1.5 w-1.5 rounded-full border border-border shrink-0" /> {e}
              </DropdownMenuItem>
            ))}
            <DropdownMenuSeparator />
            <DropdownMenuItem className="text-primary text-xs">+ New environment</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </ShowGroup>

      <ShowGroup label="Inline status bar badge">
        <div className="flex items-center gap-1.5 rounded-full bg-primary/10 border border-primary/20 px-2.5 py-1 cursor-pointer hover:bg-primary/15 transition-colors">
          <span className="h-1.5 w-1.5 rounded-full bg-primary" />
          <span className="text-[11px] text-primary font-medium">staging</span>
        </div>
        <div className="flex items-center gap-1.5 rounded-full bg-muted border border-border px-2.5 py-1 text-muted-foreground cursor-pointer hover:bg-muted/80 transition-colors">
          <span className="h-1.5 w-1.5 rounded-full border border-border" />
          <span className="text-[11px]">No environment</span>
        </div>
      </ShowGroup>
    </SectionShell>
  );
}
```

- [ ] **Step 5: Wire all 4 RocketAPI sections into `App.tsx`**

Add to `sectionMap` in `App.tsx`:

```tsx
requestbar:  lazy(() => import('./sections/RequestBarSection').then(m => ({ default: m.RequestBarSection }))),
kveditor:    lazy(() => import('./sections/KeyValueSection').then(m => ({ default: m.KeyValueSection }))),
gitstatus:   lazy(() => import('./sections/GitStatusSection').then(m => ({ default: m.GitStatusSection }))),
envswitcher: lazy(() => import('./sections/EnvSwitcherSection').then(m => ({ default: m.EnvSwitcherSection }))),
```

- [ ] **Step 6: Final smoke test — all 16 sections**

```bash
cd apps/theme-playground
yarn dev
```

Click through all 16 sections in light mode, then toggle to dark and repeat. Expected:
- No console errors in any section
- All colors update on theme toggle
- Interactive elements (checkboxes, switches, tabs, table rows, dropdowns) work
- `yarn tsc --noEmit` passes

- [ ] **Step 7: Final commit**

```bash
cd ../..
git add apps/theme-playground/src/sections/
git add apps/theme-playground/src/App.tsx
git commit -m "feat(playground): add all 16 sections — 12 shadcn + 4 RocketAPI patterns

All 16 sections interactive and theme-aware.
RocketAPI patterns: RequestBar, KeyValueEditor, GitStatus, EnvSwitcher.
Playground is complete and ready for internal design review."
```

---

## Smoke Test Checklist

After all three plans execute:

- [ ] `cd frontend && yarn build` — no errors
- [ ] `cd frontend && yarn tauri dev` — app opens; sidebar `#FAFAFD` (light) / `#191A1B` (dark)
- [ ] Toggle app theme: editor panel goes `#FFFFFF` ↔ `#121314`
- [ ] Open a request with `{{variable}}` tokens — highlight colors readable in both modes
- [ ] `cd apps/theme-playground && yarn dev` — opens at `localhost:5174`
- [ ] All 16 sections render without errors
- [ ] Light/dark toggle updates all components correctly
- [ ] TypeScript: `yarn tsc --noEmit` passes in both `frontend/` and `apps/theme-playground/`
