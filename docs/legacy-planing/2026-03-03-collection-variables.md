# Collection Variables Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add Bruno-compatible collection-level variables with `{{var}}` substitution and secret masking.

**Architecture:** Variables are stored in `collection.bru` at the collection root, parsed/written by the repository layer, exposed via two new REST endpoints, and substituted client-side in `environment.ts` alongside existing environment variables (env wins on collision).

**Tech Stack:** Go (backend repository + handler), TypeScript + React + Zustand (frontend store + UI), Radix UI Tabs (existing component).

---

## Key files to know

- Backend handler (actual, used by main.go): `backend/internal/interfaces/handlers/collection_handler.go`
- Backend repository: `backend/internal/infrastructure/repository/collection_repository.go`
- Routes: `backend/cmd/server/main.go`
- Frontend substitution util: `frontend/src/lib/environment.ts`
- Frontend types: `frontend/src/types/index.ts`
- Frontend store: `frontend/src/store/collections.ts`
- Frontend API client: `frontend/src/lib/api.ts`
- Collection overview page: `frontend/src/components/collections/CollectionOverview.tsx`
- Env variable editor reference: `frontend/src/components/collections/EnvironmentsDialog.tsx`

---

### Task 1: Backend — `CollectionVar` type + parse/format helpers

**Files:**
- Modify: `backend/internal/infrastructure/repository/collection_repository.go`
- Test: `backend/internal/infrastructure/repository/collection_vars_test.go` (new)

**Step 1: Write the failing test**

Create file `backend/internal/infrastructure/repository/collection_vars_test.go`:

```go
package repository

import (
	"testing"
)

func TestParseCollectionVars(t *testing.T) {
	content := `vars {
  baseUrl: https://api.example.com
  timeout: 30
  apiKey: secret-value
}

vars:secret [
  apiKey
]`

	vars := parseCollectionVars(content)

	if len(vars) != 3 {
		t.Fatalf("expected 3 vars, got %d", len(vars))
	}

	// baseUrl is not secret
	if vars[0].Key != "baseUrl" || vars[0].Value != "https://api.example.com" || vars[0].Secret {
		t.Errorf("unexpected baseUrl var: %+v", vars[0])
	}

	// timeout is not secret
	if vars[1].Key != "timeout" || vars[1].Value != "30" || vars[1].Secret {
		t.Errorf("unexpected timeout var: %+v", vars[1])
	}

	// apiKey is secret
	if vars[2].Key != "apiKey" || vars[2].Value != "secret-value" || !vars[2].Secret {
		t.Errorf("unexpected apiKey var: %+v", vars[2])
	}
}

func TestFormatCollectionVars(t *testing.T) {
	vars := []CollectionVar{
		{Key: "baseUrl", Value: "https://api.example.com", Enabled: true, Secret: false},
		{Key: "apiKey", Value: "secret-value", Enabled: true, Secret: true},
	}

	content := formatCollectionVars(vars)

	if !contains(content, "baseUrl: https://api.example.com") {
		t.Error("expected baseUrl in vars block")
	}
	if !contains(content, "apiKey: secret-value") {
		t.Error("expected apiKey value in vars block")
	}
	if !contains(content, "vars:secret [") {
		t.Error("expected vars:secret block")
	}
	if !contains(content, "  apiKey") {
		t.Error("expected apiKey in secret block")
	}
}

func TestParseCollectionVars_Empty(t *testing.T) {
	vars := parseCollectionVars("")
	if len(vars) != 0 {
		t.Fatalf("expected 0 vars, got %d", len(vars))
	}
}

func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(s) > 0 && containsHelper(s, substr))
}

func containsHelper(s, substr string) bool {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}
```

**Step 2: Run test to verify it fails**

```bash
cd backend && go test ./internal/infrastructure/repository/... -run TestParseCollectionVars -v
```
Expected: FAIL — `parseCollectionVars undefined`

**Step 3: Add `CollectionVar` type + helpers to `collection_repository.go`**

Add after the `ListEnvironments` function (end of file):

```go
// CollectionVar represents a single collection-level variable.
type CollectionVar struct {
	Key     string `json:"key"`
	Value   string `json:"value"`
	Enabled bool   `json:"enabled"`
	Secret  bool   `json:"secret"`
}

// parseCollectionVars parses vars {} and vars:secret [] blocks from collection.bru content.
func parseCollectionVars(content string) []CollectionVar {
	var vars []CollectionVar
	secretKeys := map[string]bool{}

	inVarsBlock := false
	inSecretBlock := false

	for _, line := range strings.Split(content, "\n") {
		trimmed := strings.TrimSpace(line)

		switch {
		case trimmed == "vars {":
			inVarsBlock = true
			inSecretBlock = false
		case trimmed == "vars:secret [":
			inSecretBlock = true
			inVarsBlock = false
		case trimmed == "}" || trimmed == "]":
			inVarsBlock = false
			inSecretBlock = false
		case inVarsBlock && trimmed != "":
			parts := strings.SplitN(trimmed, ":", 2)
			if len(parts) == 2 {
				vars = append(vars, CollectionVar{
					Key:     strings.TrimSpace(parts[0]),
					Value:   strings.TrimSpace(parts[1]),
					Enabled: true,
				})
			}
		case inSecretBlock && trimmed != "":
			secretKeys[trimmed] = true
		}
	}

	for i := range vars {
		if secretKeys[vars[i].Key] {
			vars[i].Secret = true
		}
	}

	return vars
}

// formatCollectionVars serialises vars to collection.bru format.
func formatCollectionVars(vars []CollectionVar) string {
	var sb strings.Builder
	var secretKeys []string

	sb.WriteString("vars {\n")
	for _, v := range vars {
		if v.Enabled {
			fmt.Fprintf(&sb, "  %s: %s\n", v.Key, v.Value)
			if v.Secret {
				secretKeys = append(secretKeys, v.Key)
			}
		}
	}
	sb.WriteString("}\n")

	if len(secretKeys) > 0 {
		sb.WriteString("\nvars:secret [\n")
		for _, k := range secretKeys {
			fmt.Fprintf(&sb, "  %s\n", k)
		}
		sb.WriteString("]\n")
	}

	return sb.String()
}

// ReadCollectionVars reads collection-level variables from collection.bru.
// Returns an empty slice when the file does not exist yet.
func (r *CollectionRepository) ReadCollectionVars(collectionName string) ([]CollectionVar, error) {
	path := filepath.Join(r.basePath, collectionName, "collection.bru")
	content, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return []CollectionVar{}, nil
	}
	if err != nil {
		return nil, fmt.Errorf("failed to read collection.bru: %w", err)
	}
	return parseCollectionVars(string(content)), nil
}

// WriteCollectionVars writes collection-level variables to collection.bru.
func (r *CollectionRepository) WriteCollectionVars(collectionName string, vars []CollectionVar) error {
	content := formatCollectionVars(vars)
	return r.WriteFile(collectionName, "collection.bru", []byte(content))
}
```

**Step 4: Run tests to verify they pass**

```bash
cd backend && go test ./internal/infrastructure/repository/... -run "TestParseCollectionVars|TestFormatCollectionVars" -v
```
Expected: all PASS

**Step 5: Commit**

```bash
cd backend && git add internal/infrastructure/repository/collection_repository.go internal/infrastructure/repository/collection_vars_test.go
git commit -m "feat(backend): add CollectionVar type with parse/write helpers for collection.bru"
```

---

### Task 2: Backend — handler methods `GetCollectionVars` / `SaveCollectionVars`

**Files:**
- Modify: `backend/internal/interfaces/handlers/collection_handler.go`

**Step 1: Write the failing test** (integration-style, same pattern as the existing env handler tests — but the handler test file does not exist for these endpoints, so we verify via build only; the repository unit tests from Task 1 cover the logic)

**Step 2: Add handler methods**

Add the following two methods to `collection_handler.go`, after `SaveEnvironment`:

```go
// GetCollectionVars handles GET /api/v1/collections/{name}/variables
func (h *CollectionHandler) GetCollectionVars(w http.ResponseWriter, r *http.Request) {
	if r.Method == "OPTIONS" {
		w.WriteHeader(http.StatusOK)
		return
	}

	vars := mux.Vars(r)
	name := vars["name"]

	collVars, err := h.repo.ReadCollectionVars(name)
	if err != nil {
		http.Error(w, fmt.Sprintf("Failed to read collection variables: %v", err), http.StatusInternalServerError)
		return
	}

	// Mask secret values before sending to the client.
	masked := make([]repository.CollectionVar, len(collVars))
	for i, v := range collVars {
		masked[i] = v
		if v.Secret {
			masked[i].Value = ""
		}
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"data":    masked,
		"success": true,
		"message": "Collection variables retrieved successfully",
	})
}

// SaveCollectionVars handles POST /api/v1/collections/{name}/variables
func (h *CollectionHandler) SaveCollectionVars(w http.ResponseWriter, r *http.Request) {
	if r.Method == "OPTIONS" {
		w.WriteHeader(http.StatusOK)
		return
	}

	vars := mux.Vars(r)
	name := vars["name"]

	var payload struct {
		Variables []repository.CollectionVar `json:"variables"`
	}
	if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
		http.Error(w, fmt.Sprintf("Invalid request body: %v", err), http.StatusBadRequest)
		return
	}

	if err := h.repo.WriteCollectionVars(name, payload.Variables); err != nil {
		http.Error(w, fmt.Sprintf("Failed to save collection variables: %v", err), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"success": true,
		"message": "Collection variables saved successfully",
	})
}
```

Note: `collection_handler.go` already imports `"github.com/gorilla/mux"` and `"github.com/yourusername/rocket-api/internal/infrastructure/repository"`. Verify these imports are present; add if missing.

**Step 3: Verify build**

```bash
cd backend && go build ./...
```
Expected: no errors

**Step 4: Commit**

```bash
git add backend/internal/interfaces/handlers/collection_handler.go
git commit -m "feat(backend): add GetCollectionVars and SaveCollectionVars handlers"
```

---

### Task 3: Backend — Register routes in `main.go`

**Files:**
- Modify: `backend/cmd/server/main.go`

**Step 1: Add two route registrations**

In `main.go`, find the Environment routes block (around line 137-140). Add after it:

```go
// Collection variable routes
api.HandleFunc("/collections/{name}/variables", collectionHandler.GetCollectionVars).Methods("GET", "OPTIONS")
api.HandleFunc("/collections/{name}/variables", collectionHandler.SaveCollectionVars).Methods("POST", "OPTIONS")
```

**Step 2: Verify build**

```bash
cd backend && go build ./...
```
Expected: no errors

**Step 3: Smoke-test the endpoint**

Start the backend (`go run ./cmd/server`) in one terminal, then in another:

```bash
# Should return {"data":[],"success":true,...} for any existing collection
curl http://localhost:8080/api/v1/collections/YOUR_COLLECTION_NAME/variables
```

**Step 4: Commit**

```bash
git add backend/cmd/server/main.go
git commit -m "feat(backend): register collection variable routes"
```

---

### Task 4: Frontend — Add `CollectionVar` type + API client methods

**Files:**
- Modify: `frontend/src/types/index.ts`
- Modify: `frontend/src/lib/api.ts`

**Step 1: Add `CollectionVar` to types**

In `frontend/src/types/index.ts`, add after the `EnvironmentVariable` interface:

```typescript
export interface CollectionVar {
  key: string
  value: string
  enabled: boolean
  secret: boolean
}
```

**Step 2: Add API methods to `api.ts`**

In `frontend/src/lib/api.ts`, add after `deleteEnvironment`:

```typescript
// Collection variables
async getCollectionVariables(name: string): Promise<CollectionVar[]> {
  const response = await this.client.get<ApiResponse<CollectionVar[]>>(
    `/collections/${name}/variables`
  )
  return response.data.data ?? []
}

async saveCollectionVariables(name: string, variables: CollectionVar[]): Promise<void> {
  await this.client.post(`/collections/${name}/variables`, { variables })
}
```

Add `CollectionVar` to the import from `@/types` at the top of `api.ts`:

```typescript
import { HttpRequest, HttpResponse, ApiResponse, Environment, HistoryEntry, Template, Cookie, CollectionVar } from '@/types'
```

**Step 3: Verify TypeScript build**

```bash
cd frontend && yarn build 2>&1 | tail -5
```
Expected: `✓ built in ...s`

**Step 4: Commit**

```bash
git add frontend/src/types/index.ts frontend/src/lib/api.ts
git commit -m "feat(frontend): add CollectionVar type and API client methods"
```

---

### Task 5: Frontend — Extend `collections.ts` store

**Files:**
- Modify: `frontend/src/store/collections.ts`

**Step 1: Add state + actions to the store**

In the `CollectionsState` interface, add after `activeEnvironment`:

```typescript
collectionVariables: CollectionVar[]
fetchCollectionVariables: (name: string) => Promise<void>
saveCollectionVariables: (name: string, vars: CollectionVar[]) => Promise<void>
```

In the store implementation, add the initial value after `activeEnvironment: null`:

```typescript
collectionVariables: [],
```

Add these action implementations after `deleteEnvironment`:

```typescript
fetchCollectionVariables: async (name: string) => {
  try {
    const vars = await apiService.getCollectionVariables(name)
    set({ collectionVariables: vars })
  } catch (error) {
    console.error('Failed to fetch collection variables:', error)
  }
},

saveCollectionVariables: async (name: string, vars: CollectionVar[]) => {
  await apiService.saveCollectionVariables(name, vars)
  set({ collectionVariables: vars })
},
```

In `setActiveCollection`, call `fetchCollectionVariables` alongside `fetchEnvironments`:

Find this block inside `setActiveCollection`:
```typescript
setActiveCollection: (collection: CollectionSummary | null) => {
  set({ activeCollection: collection, activeEnvironment: null })
  if (collection) {
    get().fetchEnvironments(collection.name).then(() => {
```

Add a call to `fetchCollectionVariables` right after the `fetchEnvironments` call:
```typescript
setActiveCollection: (collection: CollectionSummary | null) => {
  set({ activeCollection: collection, activeEnvironment: null, collectionVariables: [] })
  if (collection) {
    get().fetchEnvironments(collection.name).then(() => {
      // Restore last-used environment for this collection.
      const savedName = localStorage.getItem(`rocket-api:active-env:${collection.name}`)
      if (savedName) {
        const env = get().environments.find(e => e.name === savedName)
        if (env) set({ activeEnvironment: env })
      }
    })
    get().fetchCollectionVariables(collection.name)
  }
},
```

Add `CollectionVar` to the import at the top of `collections.ts`:

```typescript
import { Environment, CollectionVar } from '@/types'
```

**Step 2: Verify TypeScript build**

```bash
cd frontend && yarn build 2>&1 | tail -5
```
Expected: `✓ built in ...s`

**Step 3: Commit**

```bash
git add frontend/src/store/collections.ts
git commit -m "feat(frontend): add collectionVariables state and fetch/save actions to collections store"
```

---

### Task 6: Frontend — Extend `environment.ts` substitution for collection vars

**Files:**
- Modify: `frontend/src/lib/environment.ts`

**Step 1: Update `substituteRequestVariables` signature**

Replace the existing `substituteRequestVariables` function with this version that accepts an optional `collectionVars` parameter:

```typescript
/**
 * Substitutes variables in URL, headers, and body.
 * Environment variables override collection variables on key collision.
 */
export function substituteRequestVariables(
  url: string,
  headers: Array<{ key: string; value: string; enabled: boolean }>,
  body: string,
  environment: import('@/types').Environment | null,
  collectionVars?: import('@/types').CollectionVar[]
): {
  url: string
  headers: Array<{ key: string; value: string; enabled: boolean }>
  body: string
} {
  // Build merged variable map: collection vars first, env vars override.
  const merged: Record<string, string> = {}

  if (collectionVars) {
    for (const v of collectionVars) {
      if (v.enabled && v.key) {
        merged[v.key] = v.value
      }
    }
  }

  if (environment?.variables) {
    for (const v of environment.variables) {
      if (v.enabled) {
        merged[v.key] = v.value
      }
    }
  }

  const substitute = (text: string) =>
    text.replace(/\{\{(\w+)\}\}/g, (match, varName) =>
      merged[varName] !== undefined ? merged[varName] : match
    )

  return {
    url: substitute(url),
    headers: headers.map(h => ({
      ...h,
      key: substitute(h.key),
      value: substitute(h.value),
    })),
    body: substitute(body),
  }
}
```

Note: `substituteVariables` (single-string helper) is unchanged — it is still used by `validateVariables` and potentially other callers.

**Step 2: Verify TypeScript build**

```bash
cd frontend && yarn build 2>&1 | tail -5
```
Expected: `✓ built in ...s`

**Step 3: Commit**

```bash
git add frontend/src/lib/environment.ts
git commit -m "feat(frontend): extend substituteRequestVariables to merge collection vars"
```

---

### Task 7: Frontend — Wire collection vars into `RequestBuilder.tsx`

**Files:**
- Modify: `frontend/src/components/request-builder/RequestBuilder.tsx`

**Step 1: Read the file first**

Read lines 1–30 and 190–210 to confirm current imports and the `substituteRequestVariables` call site.

**Step 2: Add `collectionVariables` to the store subscription**

Find where `activeEnvironment` is read from `useCollectionsStore`:

```typescript
const { activeEnvironment } = useCollectionsStore.getState()
```

Change to:

```typescript
const { activeEnvironment, collectionVariables } = useCollectionsStore.getState()
```

**Step 3: Pass `collectionVariables` to `substituteRequestVariables`**

Find the existing call:

```typescript
const substituted = substituteRequestVariables(
  url,
  headers,
  body.content,
  activeEnvironment
)
```

Change to:

```typescript
const substituted = substituteRequestVariables(
  url,
  headers,
  body.content,
  activeEnvironment,
  collectionVariables
)
```

**Step 4: Verify TypeScript build**

```bash
cd frontend && yarn build 2>&1 | tail -5
```
Expected: `✓ built in ...s`

**Step 5: Commit**

```bash
git add frontend/src/components/request-builder/RequestBuilder.tsx
git commit -m "feat(frontend): pass collection vars to substituteRequestVariables in RequestBuilder"
```

---

### Task 8: Frontend — `CollectionVariablesEditor` component

**Files:**
- Create: `frontend/src/components/collections/CollectionVariablesEditor.tsx`

**Step 1: Create the component**

```typescript
import { useState } from 'react'
import { useCollectionsStore } from '@/store/collections'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Plus, Trash2, Lock, Eye, EyeOff, Loader2 } from 'lucide-react'
import type { CollectionVar } from '@/types'

export function CollectionVariablesEditor() {
  const { activeCollection, collectionVariables, saveCollectionVariables } = useCollectionsStore()

  const [editingVars, setEditingVars] = useState<CollectionVar[]>(() =>
    collectionVariables.map(v => ({ ...v }))
  )
  const [isDirty, setIsDirty] = useState(false)
  const [isSaving, setIsSaving] = useState(false)
  const [revealedSecrets, setRevealedSecrets] = useState<Set<number>>(new Set())

  const handleVarChange = (index: number, field: keyof CollectionVar, value: string | boolean) => {
    setEditingVars(vars => vars.map((v, i) => i === index ? { ...v, [field]: value } : v))
    setIsDirty(true)
  }

  const handleAddVar = () => {
    setEditingVars(vars => [...vars, { key: '', value: '', enabled: true, secret: false }])
    setIsDirty(true)
  }

  const handleDeleteVar = (index: number) => {
    setEditingVars(vars => vars.filter((_, i) => i !== index))
    setIsDirty(true)
  }

  const toggleRevealSecret = (index: number) => {
    setRevealedSecrets(prev => {
      const next = new Set(prev)
      if (next.has(index)) next.delete(index); else next.add(index)
      return next
    })
  }

  const handleSave = async () => {
    if (!activeCollection) return
    setIsSaving(true)
    try {
      await saveCollectionVariables(activeCollection.name, editingVars)
      setIsDirty(false)
    } finally {
      setIsSaving(false)
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <p className="text-xs text-muted-foreground">
          Use <code className="bg-muted px-1 rounded">{'{{variableName}}'}</code> in requests.
          Environment variables override these on collision.
        </p>
        <Button
          size="sm"
          onClick={handleSave}
          disabled={!isDirty || isSaving}
          className="h-7 text-xs"
        >
          {isSaving ? <Loader2 className="h-3.5 w-3.5 mr-1 animate-spin" /> : null}
          Save
        </Button>
      </div>

      {/* Header row */}
      <div className="grid grid-cols-[1fr_1fr_auto_auto_auto] gap-2 px-1">
        <span className="text-xs font-medium text-muted-foreground">Key</span>
        <span className="text-xs font-medium text-muted-foreground">Value</span>
        <span className="text-xs font-medium text-muted-foreground w-8 text-center">Secret</span>
        <span className="text-xs font-medium text-muted-foreground w-8 text-center">On</span>
        <span className="w-7" />
      </div>

      {/* Variable rows */}
      <div className="space-y-1.5">
        {editingVars.map((v, i) => (
          <div key={i} className="grid grid-cols-[1fr_1fr_auto_auto_auto] gap-2 items-center">
            <Input
              value={v.key}
              onChange={e => handleVarChange(i, 'key', e.target.value)}
              placeholder="variable_name"
              className="h-7 text-xs font-mono"
            />
            <div className="relative">
              <Input
                value={v.value}
                onChange={e => handleVarChange(i, 'value', e.target.value)}
                placeholder="value"
                type={v.secret && !revealedSecrets.has(i) ? 'password' : 'text'}
                className="h-7 text-xs font-mono pr-7"
              />
              {v.secret && (
                <button
                  type="button"
                  className="absolute right-1.5 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                  onClick={() => toggleRevealSecret(i)}
                  title={revealedSecrets.has(i) ? 'Hide value' : 'Reveal value'}
                >
                  {revealedSecrets.has(i) ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
                </button>
              )}
            </div>
            <button
              type="button"
              title="Mark as secret"
              className={`h-7 w-8 flex items-center justify-center rounded transition-colors ${
                v.secret ? 'text-amber-500' : 'text-muted-foreground hover:text-foreground'
              }`}
              onClick={() => handleVarChange(i, 'secret', !v.secret)}
            >
              <Lock className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              title={v.enabled ? 'Disable variable' : 'Enable variable'}
              className={`h-7 w-8 flex items-center justify-center rounded text-xs font-medium transition-colors ${
                v.enabled ? 'text-foreground' : 'text-muted-foreground'
              }`}
              onClick={() => handleVarChange(i, 'enabled', !v.enabled)}
            >
              {v.enabled ? 'On' : 'Off'}
            </button>
            <button
              type="button"
              className="h-7 w-7 flex items-center justify-center text-muted-foreground hover:text-destructive transition-colors"
              onClick={() => handleDeleteVar(i)}
              title="Delete variable"
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          </div>
        ))}
      </div>

      <Button
        variant="ghost"
        size="sm"
        className="h-7 text-xs w-full justify-start"
        onClick={handleAddVar}
      >
        <Plus className="h-3.5 w-3.5 mr-1" />
        Add variable
      </Button>
    </div>
  )
}
```

**Step 2: Verify TypeScript build**

```bash
cd frontend && yarn build 2>&1 | tail -5
```
Expected: `✓ built in ...s`

**Step 3: Commit**

```bash
git add frontend/src/components/collections/CollectionVariablesEditor.tsx
git commit -m "feat(frontend): add CollectionVariablesEditor component"
```

---

### Task 9: Frontend — Add "Variables" tab to `CollectionOverview.tsx`

**Files:**
- Modify: `frontend/src/components/collections/CollectionOverview.tsx`

**Step 1: Add imports**

Add to the import list at the top of `CollectionOverview.tsx`:

```typescript
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs'
import { CollectionVariablesEditor } from './CollectionVariablesEditor'
```

**Step 2: Wrap the existing overview content in a tab structure**

Find the outermost `return` statement — currently:

```typescript
return (
  <ScrollArea className="flex-1">
    <div className="p-6 space-y-6 max-w-4xl mx-auto">
      {/* ... all existing content ... */}
    </div>
  </ScrollArea>
)
```

Wrap it with tabs so "Overview" shows the existing content and "Variables" shows the editor:

```typescript
return (
  <div className="flex-1 flex flex-col overflow-hidden">
    <Tabs defaultValue="overview" className="flex-1 flex flex-col overflow-hidden">
      <div className="border-b border-border px-6 shrink-0">
        <TabsList className="h-9 bg-transparent p-0 gap-0">
          <TabsTrigger
            value="overview"
            className="h-9 rounded-none border-b-2 border-transparent data-[state=active]:border-foreground data-[state=active]:bg-transparent px-3 text-xs"
          >
            Overview
          </TabsTrigger>
          <TabsTrigger
            value="variables"
            className="h-9 rounded-none border-b-2 border-transparent data-[state=active]:border-foreground data-[state=active]:bg-transparent px-3 text-xs"
          >
            Variables
          </TabsTrigger>
        </TabsList>
      </div>

      <TabsContent value="overview" className="flex-1 overflow-hidden mt-0">
        <ScrollArea className="h-full">
          <div className="p-6 space-y-6 max-w-4xl mx-auto">
            {/* --- paste all existing content here (Header, Stats Bar, Two-Column, Recent Activity) --- */}
          </div>
        </ScrollArea>
      </TabsContent>

      <TabsContent value="variables" className="flex-1 overflow-hidden mt-0">
        <ScrollArea className="h-full">
          <div className="p-6 max-w-2xl mx-auto">
            <CollectionVariablesEditor />
          </div>
        </ScrollArea>
      </TabsContent>
    </Tabs>
  </div>
)
```

The existing JSX content (from `{/* Header */}` through `{/* Recent Activity */}` card) goes verbatim inside the `TabsContent value="overview"` div. Do not change any of that content.

**Step 3: Verify TypeScript build**

```bash
cd frontend && yarn build 2>&1 | tail -5
```
Expected: `✓ built in ...s`

**Step 4: Manual verification**

1. Open a collection — confirm "Overview" and "Variables" tabs appear.
2. Click "Variables" tab — confirm editor renders.
3. Add a variable `baseUrl = https://api.example.com`, save — verify `collection.bru` appears in `~/.rocket-api/collections/<name>/`.
4. Create a request using `{{baseUrl}}/users` — confirm the URL resolves when sent.
5. Add a secret variable, save — confirm value is masked with `●●●●` on next load.

**Step 5: Commit**

```bash
git add frontend/src/components/collections/CollectionOverview.tsx
git commit -m "feat(frontend): add Variables tab to CollectionOverview with CollectionVariablesEditor"
```

---

## Done

After all 9 tasks are complete and manual verification passes, the collection variables feature is ready.

**What was built:**
- `collection.bru` file at collection root stores vars in Bruno-compatible format
- `GET /api/v1/collections/{name}/variables` — returns vars, secrets masked
- `POST /api/v1/collections/{name}/variables` — saves vars
- `{{var}}` placeholders in URL/headers/body/query resolve at send time; env vars override collection vars
- Variables tab in CollectionOverview with key/value/secret/enabled editor
