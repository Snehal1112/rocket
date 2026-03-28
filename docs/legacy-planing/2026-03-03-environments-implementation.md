# Environments Feature Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement full per-collection environment management — create, edit, delete, select — with secret variable support, a Bruno-style dialog, and an inline env selector in the request builder. Variable substitution is already wired; this plan makes CRUD fully functional.

**Architecture:** Backend gains secret file support (`name.env.secret`) and a DELETE endpoint. Frontend replaces the stubbed right-side `EnvironmentsPanel` with a proper `EnvironmentsDialog` modal and moves the active-env selector into the `RequestBuilder` URL bar. Active environment is persisted to `localStorage` per collection.

**Tech Stack:** Go (backend), React 19 + TypeScript, Zustand 4, Tailwind CSS, shadcn/ui (`Dialog`, `Select`, `Button`, `Input` — all already installed)

---

### Task 1: Update backend — secret file support in `ReadEnvironment` / `WriteEnvironment`

**Files:**
- Modify: `backend/internal/infrastructure/repository/collection_repository.go`

The backend's `Environment` struct (used in the repository layer) is currently `map[string]string` — no `enabled` or `secret` flags. We need a richer type and two-file read/write.

**Step 1: Add a new `EnvVariable` struct and update `Environment`**

Find and replace the `Environment` struct (currently defined inline only through `map[string]string`). Add at the top of `collection_repository.go` (after the `package` and `import` block):

```go
// EnvVariable represents a single environment variable with metadata.
type EnvVariable struct {
	Key     string `json:"key"`
	Value   string `json:"value"`
	Enabled bool   `json:"enabled"`
	Secret  bool   `json:"secret"`
}

// Environment represents a named set of variables for a collection.
type Environment struct {
	Name      string        `json:"name"`
	Variables []EnvVariable `json:"variables"`
}
```

**Step 2: Replace `ReadEnvironment`**

Find:
```go
// ReadEnvironment reads an environment file
func (r *CollectionRepository) ReadEnvironment(collectionName, envName string) (*Environment, error) {
	envPath := filepath.Join(r.basePath, collectionName, "environments", envName+".env")
	content, err := os.ReadFile(envPath)
	if err != nil {
		return nil, fmt.Errorf("failed to read environment: %w", err)
	}

	env := &Environment{
		Name:      envName,
		Variables: make(map[string]string),
	}

	for line := range strings.SplitSeq(string(content), "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}

		parts := strings.SplitN(line, "=", 2)
		if len(parts) == 2 {
			key := strings.TrimSpace(parts[0])
			value := strings.TrimSpace(parts[1])
			env.Variables[key] = value
		}
	}

	return env, nil
}
```

Replace with:
```go
// ReadEnvironment reads an environment from its .env and .env.secret files.
func (r *CollectionRepository) ReadEnvironment(collectionName, envName string) (*Environment, error) {
	env := &Environment{Name: envName}

	readFile := func(path string, secret bool) {
		content, err := os.ReadFile(path)
		if err != nil {
			return // File may not exist; that's fine.
		}
		for line := range strings.SplitSeq(string(content), "\n") {
			line = strings.TrimSpace(line)
			if line == "" || strings.HasPrefix(line, "#") {
				continue
			}
			parts := strings.SplitN(line, "=", 2)
			if len(parts) == 2 {
				env.Variables = append(env.Variables, EnvVariable{
					Key:     strings.TrimSpace(parts[0]),
					Value:   strings.TrimSpace(parts[1]),
					Enabled: true,
					Secret:  secret,
				})
			}
		}
	}

	base := filepath.Join(r.basePath, collectionName, "environments")
	readFile(filepath.Join(base, envName+".env"), false)
	readFile(filepath.Join(base, envName+".env.secret"), true)

	if len(env.Variables) == 0 {
		// Neither file exists — environment not found.
		mainPath := filepath.Join(base, envName+".env")
		if _, err := os.Stat(mainPath); os.IsNotExist(err) {
			return nil, fmt.Errorf("environment %q not found", envName)
		}
	}

	return env, nil
}
```

**Step 3: Replace `WriteEnvironment`**

Find:
```go
// WriteEnvironment writes an environment file
func (r *CollectionRepository) WriteEnvironment(collectionName string, env *Environment) error {
	var content strings.Builder
	for key, value := range env.Variables {
		fmt.Fprintf(&content, "%s=%s\n", key, value)
	}

	envPath := filepath.Join("environments", env.Name+".env")
	return r.WriteFile(collectionName, envPath, []byte(content.String()))
}
```

Replace with:
```go
// WriteEnvironment writes enabled variables to .env (non-secrets) and .env.secret (secrets).
func (r *CollectionRepository) WriteEnvironment(collectionName string, env *Environment) error {
	var plain, secret strings.Builder
	for _, v := range env.Variables {
		if !v.Enabled {
			continue
		}
		if v.Secret {
			fmt.Fprintf(&secret, "%s=%s\n", v.Key, v.Value)
		} else {
			fmt.Fprintf(&plain, "%s=%s\n", v.Key, v.Value)
		}
	}

	if err := r.WriteFile(collectionName, filepath.Join("environments", env.Name+".env"), []byte(plain.String())); err != nil {
		return err
	}
	return r.WriteFile(collectionName, filepath.Join("environments", env.Name+".env.secret"), []byte(secret.String()))
}
```

**Step 4: Add `DeleteEnvironment`**

Add after `WriteEnvironment`:
```go
// DeleteEnvironment removes both the .env and .env.secret files for an environment.
func (r *CollectionRepository) DeleteEnvironment(collectionName, envName string) error {
	base := filepath.Join(r.basePath, collectionName, "environments")
	os.Remove(filepath.Join(base, envName+".env.secret")) // Ignore error — may not exist.
	if err := os.Remove(filepath.Join(base, envName+".env")); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("failed to delete environment: %w", err)
	}
	return nil
}
```

**Step 5: Build the backend to verify no compile errors**

```bash
cd /home/numericlabs/data/rocket-api/backend && go build ./...
```

Expected: no output (clean build).

**Step 6: Commit**

```bash
cd /home/numericlabs/data/rocket-api
git add backend/internal/infrastructure/repository/collection_repository.go
git commit -m "feat(env): secret file support and DeleteEnvironment in repository"
```

---

### Task 2: Update backend handlers — new env shape and DELETE endpoint

**Files:**
- Modify: `backend/internal/interfaces/handlers/collection_handler.go`
- Modify: `backend/internal/interfaces/handlers/routes.go`

**Step 1: Update `SaveEnvironment` handler payload type**

Find the `SaveEnvironment` handler payload struct:
```go
	var payload struct {
		Collection string                       `json:"collection"`
		Environment *repository.Environment     `json:"environment"`
	}
```

This already uses `*repository.Environment` — no change needed. The shape is now richer (variables as array) but the struct reference stays the same.

**Step 2: Add `DeleteEnvironment` handler**

Find the end of `collection_handler.go` (after `SaveEnvironment`). Add:
```go
// DeleteEnvironment handles DELETE /api/v1/environments
func (h *CollectionHandler) DeleteEnvironment(w http.ResponseWriter, r *http.Request) {
	if r.Method == "OPTIONS" {
		w.WriteHeader(http.StatusOK)
		return
	}

	query := r.URL.Query()
	collection := query.Get("collection")
	envName := query.Get("name")

	if collection == "" || envName == "" {
		http.Error(w, "Collection and environment name are required", http.StatusBadRequest)
		return
	}

	if err := h.repo.DeleteEnvironment(collection, envName); err != nil {
		http.Error(w, fmt.Sprintf("Failed to delete environment: %v", err), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"success": true,
		"message": "Environment deleted successfully",
	})
}
```

**Step 3: Register the DELETE route**

Open `backend/internal/interfaces/handlers/routes.go`. The current routes are registered on a plain `*mux.Router` using top-level handler functions (not methods). The environment handlers are in `CollectionHandler`. Check how environments is registered — currently it is `GetEnvironmentsHandler` (a top-level function), not a method. We need to align with how the collection handler is wired.

Read `backend/cmd/server/main.go` to see how `CollectionHandler` is created and used:

```bash
grep -n "CollectionHandler\|RegisterRoutes\|environments" /home/numericlabs/data/rocket-api/backend/cmd/server/main.go | head -20
grep -n "CollectionHandler\|environments" /home/numericlabs/data/rocket-api/backend/internal/interfaces/handlers/routes.go
```

If `CollectionHandler` is instantiated in `main.go` and routes are registered separately, update `routes.go` to register the DELETE route alongside the existing environment routes. Find:

```go
r.HandleFunc("/environments", GetEnvironmentsHandler).Methods("GET", "OPTIONS")
```

Replace with:

```go
r.HandleFunc("/environments", GetEnvironmentsHandler).Methods("GET", "OPTIONS")
r.HandleFunc("/environments", DeleteEnvironmentHandler).Methods("DELETE", "OPTIONS")
```

Then add a `DeleteEnvironmentHandler` top-level function (matching the pattern of existing top-level handlers) OR wire the method — check the existing pattern first and follow it exactly.

**Step 4: Build the backend**

```bash
cd /home/numericlabs/data/rocket-api/backend && go build ./...
```

Expected: clean build.

**Step 5: Commit**

```bash
cd /home/numericlabs/data/rocket-api
git add backend/internal/interfaces/handlers/collection_handler.go \
        backend/internal/interfaces/handlers/routes.go
git commit -m "feat(env): add DeleteEnvironment handler and route"
```

---

### Task 3: Update frontend types and API client

**Files:**
- Modify: `frontend/src/types/index.ts`
- Modify: `frontend/src/lib/api.ts`

**Step 1: Add `secret` flag to `EnvironmentVariable`**

In `frontend/src/types/index.ts`, find:
```typescript
export interface EnvironmentVariable {
  key: string
  value: string
  enabled: boolean
}
```

Replace with:
```typescript
export interface EnvironmentVariable {
  key: string
  value: string
  enabled: boolean
  secret: boolean
}
```

**Step 2: Add `deleteEnvironment` to the API client**

In `frontend/src/lib/api.ts`, find:
```typescript
  async saveEnvironment(collection: string, environment: Environment): Promise<void> {
    await this.client.post('/environments', {
      collection,
      environment
    })
  }
```

Add immediately after:
```typescript
  async deleteEnvironment(collection: string, name: string): Promise<void> {
    await this.client.delete('/environments', {
      params: { collection, name }
    })
  }
```

**Step 3: Build to verify no TypeScript errors**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn build 2>&1 | tail -5
```

Expected: `✓ built in X.XXs`

**Step 4: Commit**

```bash
cd /home/numericlabs/data/rocket-api
git add frontend/src/types/index.ts frontend/src/lib/api.ts
git commit -m "feat(env): add secret flag to EnvironmentVariable and deleteEnvironment API method"
```

---

### Task 4: Update collections store — wire CRUD and add localStorage persistence

**Files:**
- Modify: `frontend/src/store/collections.ts`

**Step 1: Add new action signatures to the `CollectionsState` interface**

Find:
```typescript
  fetchEnvironments: (collection: string) => Promise<void>
  setActiveEnvironment: (environment: Environment | null) => void
```

Replace with:
```typescript
  fetchEnvironments: (collection: string) => Promise<void>
  setActiveEnvironment: (environment: Environment | null) => void
  createEnvironment: (collectionName: string, name: string) => Promise<void>
  saveEnvironment: (collectionName: string, env: Environment) => Promise<void>
  deleteEnvironment: (collectionName: string, name: string) => Promise<void>
```

**Step 2: Update `setActiveCollection` to restore persisted active env**

Find:
```typescript
  setActiveCollection: (collection: CollectionSummary | null) => {
    set({ activeCollection: collection })
    if (collection) {
      get().fetchEnvironments(collection.name)
    }
  },
```

Replace with:
```typescript
  setActiveCollection: (collection: CollectionSummary | null) => {
    set({ activeCollection: collection, activeEnvironment: null })
    if (collection) {
      get().fetchEnvironments(collection.name).then(() => {
        // Restore last-used environment for this collection.
        const savedName = localStorage.getItem(`rocket-api:active-env:${collection.name}`)
        if (savedName) {
          const env = get().environments.find(e => e.name === savedName)
          if (env) set({ activeEnvironment: env })
        }
      })
    }
  },
```

**Step 3: Update `setActiveEnvironment` to persist to localStorage**

Find:
```typescript
  setActiveEnvironment: (environment: Environment | null) => {
    set({ activeEnvironment: environment })
  },
```

Replace with:
```typescript
  setActiveEnvironment: (environment: Environment | null) => {
    set({ activeEnvironment: environment })
    const collection = get().activeCollection
    if (!collection) return
    if (environment) {
      localStorage.setItem(`rocket-api:active-env:${collection.name}`, environment.name)
    } else {
      localStorage.removeItem(`rocket-api:active-env:${collection.name}`)
    }
  },
```

**Step 4: Add `createEnvironment`, `saveEnvironment`, `deleteEnvironment` actions**

Find:
```typescript
  importBruno: async (file: File, name?: string) => {
```

Add before it:
```typescript
  createEnvironment: async (collectionName: string, name: string) => {
    const newEnv: Environment = { id: crypto.randomUUID(), name, variables: [] }
    await apiService.saveEnvironment(collectionName, newEnv)
    await get().fetchEnvironments(collectionName)
    // Auto-select the newly created environment.
    const created = get().environments.find(e => e.name === name)
    if (created) get().setActiveEnvironment(created)
  },

  saveEnvironment: async (collectionName: string, env: Environment) => {
    await apiService.saveEnvironment(collectionName, env)
    await get().fetchEnvironments(collectionName)
    // Keep activeEnvironment in sync with the refreshed list.
    if (get().activeEnvironment?.name === env.name) {
      const updated = get().environments.find(e => e.name === env.name) ?? null
      set({ activeEnvironment: updated })
    }
  },

  deleteEnvironment: async (collectionName: string, name: string) => {
    await apiService.deleteEnvironment(collectionName, name)
    if (get().activeEnvironment?.name === name) {
      get().setActiveEnvironment(null)
    }
    await get().fetchEnvironments(collectionName)
  },

```

**Step 5: Build to verify**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn build 2>&1 | tail -5
```

Expected: `✓ built in X.XXs`

**Step 6: Commit**

```bash
cd /home/numericlabs/data/rocket-api
git add frontend/src/store/collections.ts
git commit -m "feat(env): wire createEnvironment, saveEnvironment, deleteEnvironment and add localStorage persistence"
```

---

### Task 5: Build `EnvironmentsDialog` component

**Files:**
- Create: `frontend/src/components/collections/EnvironmentsDialog.tsx`

This is the main management UI. Two-column Dialog: env list on the left, variable editor on the right.

**Step 1: Create the file**

Create `frontend/src/components/collections/EnvironmentsDialog.tsx` with the following complete implementation:

```tsx
import { useState } from 'react'
import { useCollectionsStore } from '@/store/collections'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Plus, Trash2, Lock, Loader2, Eye, EyeOff } from 'lucide-react'
import type { Environment, EnvironmentVariable } from '@/types'

interface EnvironmentsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function EnvironmentsDialog({ open, onOpenChange }: EnvironmentsDialogProps) {
  const {
    environments,
    activeCollection,
    createEnvironment,
    saveEnvironment,
    deleteEnvironment,
  } = useCollectionsStore()

  const [selectedEnvName, setSelectedEnvName] = useState<string | null>(null)
  const [editingVars, setEditingVars] = useState<EnvironmentVariable[]>([])
  const [isDirty, setIsDirty] = useState(false)
  const [isSaving, setIsSaving] = useState(false)
  const [newEnvName, setNewEnvName] = useState('')
  const [isCreating, setIsCreating] = useState(false)
  const [deleteCandidate, setDeleteCandidate] = useState<string | null>(null)
  const [revealedSecrets, setRevealedSecrets] = useState<Set<number>>(new Set())

  const selectedEnv = environments.find(e => e.name === selectedEnvName) ?? null

  const handleSelectEnv = (env: Environment) => {
    setSelectedEnvName(env.name)
    setEditingVars(env.variables.map(v => ({ ...v })))
    setIsDirty(false)
    setRevealedSecrets(new Set())
  }

  const handleVarChange = (index: number, field: keyof EnvironmentVariable, value: string | boolean) => {
    const updated = editingVars.map((v, i) => i === index ? { ...v, [field]: value } : v)
    setEditingVars(updated)
    setIsDirty(true)
  }

  const handleAddVar = () => {
    setEditingVars([...editingVars, { key: '', value: '', enabled: true, secret: false }])
    setIsDirty(true)
  }

  const handleDeleteVar = (index: number) => {
    setEditingVars(editingVars.filter((_, i) => i !== index))
    setIsDirty(true)
  }

  const handleSave = async () => {
    if (!activeCollection || !selectedEnv) return
    setIsSaving(true)
    try {
      await saveEnvironment(activeCollection.name, { ...selectedEnv, variables: editingVars })
      setIsDirty(false)
    } finally {
      setIsSaving(false)
    }
  }

  const handleCreateEnv = async () => {
    const trimmed = newEnvName.trim()
    if (!trimmed || !activeCollection) return
    setIsCreating(true)
    try {
      await createEnvironment(activeCollection.name, trimmed)
      setNewEnvName('')
      // Select the new environment in the editor.
      setSelectedEnvName(trimmed)
      setEditingVars([])
      setIsDirty(false)
    } finally {
      setIsCreating(false)
    }
  }

  const handleDeleteEnv = async () => {
    if (!deleteCandidate || !activeCollection) return
    await deleteEnvironment(activeCollection.name, deleteCandidate)
    if (selectedEnvName === deleteCandidate) {
      setSelectedEnvName(null)
      setEditingVars([])
    }
    setDeleteCandidate(null)
  }

  const toggleRevealSecret = (index: number) => {
    setRevealedSecrets(prev => {
      const next = new Set(prev)
      next.has(index) ? next.delete(index) : next.add(index)
      return next
    })
  }

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="max-w-3xl h-[520px] flex flex-col gap-0 p-0 overflow-hidden">
          <DialogHeader className="px-5 py-4 border-b shrink-0">
            <DialogTitle className="text-base font-semibold">Manage Environments</DialogTitle>
          </DialogHeader>

          <div className="flex flex-1 overflow-hidden">
            {/* Left column — env list */}
            <div className="w-48 border-r flex flex-col shrink-0">
              <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
                {environments.map(env => (
                  <div
                    key={env.name}
                    className={`flex items-center group rounded px-2 py-1.5 cursor-pointer text-sm ${
                      selectedEnvName === env.name
                        ? 'bg-accent text-accent-foreground'
                        : 'hover:bg-muted/50 text-foreground'
                    }`}
                    onClick={() => handleSelectEnv(env)}
                  >
                    <span className="flex-1 truncate">{env.name}</span>
                    <button
                      type="button"
                      className="opacity-0 group-hover:opacity-100 transition-opacity ml-1 p-0.5 rounded hover:bg-destructive/10 text-destructive shrink-0"
                      onClick={(e) => { e.stopPropagation(); setDeleteCandidate(env.name) }}
                      title={`Delete ${env.name}`}
                    >
                      <Trash2 className="h-3 w-3" />
                    </button>
                  </div>
                ))}
              </div>

              {/* New environment input */}
              <div className="border-t p-2 space-y-1.5">
                <Input
                  value={newEnvName}
                  onChange={(e) => setNewEnvName(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && handleCreateEnv()}
                  placeholder="New environment..."
                  className="h-7 text-xs"
                  disabled={isCreating || !activeCollection}
                />
                <Button
                  size="sm"
                  variant="outline"
                  className="w-full h-7 text-xs"
                  onClick={handleCreateEnv}
                  disabled={!newEnvName.trim() || isCreating || !activeCollection}
                >
                  {isCreating ? (
                    <Loader2 className="h-3 w-3 mr-1.5 animate-spin" />
                  ) : (
                    <Plus className="h-3 w-3 mr-1.5" />
                  )}
                  Add
                </Button>
              </div>
            </div>

            {/* Right column — variable editor */}
            <div className="flex-1 flex flex-col overflow-hidden">
              {selectedEnv ? (
                <>
                  {/* Table header */}
                  <div className="grid grid-cols-[auto_1fr_1fr_auto_auto] gap-2 px-4 py-2 border-b bg-muted/30 text-xs font-medium text-muted-foreground">
                    <span className="w-5 text-center">ON</span>
                    <span>KEY</span>
                    <span>VALUE</span>
                    <span className="w-6 text-center">
                      <Lock className="h-3 w-3 inline" />
                    </span>
                    <span className="w-6" />
                  </div>

                  {/* Variable rows */}
                  <div className="flex-1 overflow-y-auto">
                    {editingVars.length === 0 ? (
                      <p className="text-xs text-muted-foreground text-center py-6">No variables. Add one below.</p>
                    ) : (
                      editingVars.map((v, i) => (
                        <div
                          key={i}
                          className="grid grid-cols-[auto_1fr_1fr_auto_auto] gap-2 px-4 py-1.5 border-b items-center group"
                        >
                          {/* Enabled toggle */}
                          <input
                            type="checkbox"
                            checked={v.enabled}
                            onChange={(e) => handleVarChange(i, 'enabled', e.target.checked)}
                            className="w-4 h-4 rounded accent-orange-500"
                          />
                          {/* Key */}
                          <Input
                            value={v.key}
                            onChange={(e) => handleVarChange(i, 'key', e.target.value)}
                            placeholder="KEY"
                            className="h-7 text-xs font-mono"
                          />
                          {/* Value */}
                          <div className="relative">
                            <Input
                              type={v.secret && !revealedSecrets.has(i) ? 'password' : 'text'}
                              value={v.value}
                              onChange={(e) => handleVarChange(i, 'value', e.target.value)}
                              placeholder="value"
                              className="h-7 text-xs font-mono pr-7"
                            />
                            {v.secret && (
                              <button
                                type="button"
                                className="absolute right-1.5 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                                onClick={() => toggleRevealSecret(i)}
                              >
                                {revealedSecrets.has(i) ? (
                                  <EyeOff className="h-3 w-3" />
                                ) : (
                                  <Eye className="h-3 w-3" />
                                )}
                              </button>
                            )}
                          </div>
                          {/* Secret toggle */}
                          <button
                            type="button"
                            onClick={() => handleVarChange(i, 'secret', !v.secret)}
                            title={v.secret ? 'Secret (click to make plain)' : 'Plain (click to make secret)'}
                            className={`w-6 h-6 flex items-center justify-center rounded border transition-colors ${
                              v.secret
                                ? 'border-orange-400 text-orange-500 bg-orange-50 dark:bg-orange-500/10'
                                : 'border-transparent text-muted-foreground hover:border-border'
                            }`}
                          >
                            <Lock className="h-3 w-3" />
                          </button>
                          {/* Delete */}
                          <button
                            type="button"
                            onClick={() => handleDeleteVar(i)}
                            className="w-6 h-6 flex items-center justify-center rounded opacity-0 group-hover:opacity-100 transition-opacity text-muted-foreground hover:text-destructive"
                          >
                            <Trash2 className="h-3 w-3" />
                          </button>
                        </div>
                      ))
                    )}
                  </div>

                  {/* Footer */}
                  <div className="border-t px-4 py-3 flex items-center justify-between bg-muted/20 shrink-0">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={handleAddVar}
                      className="text-xs h-7"
                    >
                      <Plus className="h-3 w-3 mr-1.5" />
                      Add Variable
                    </Button>
                    <Button
                      size="sm"
                      onClick={handleSave}
                      disabled={!isDirty || isSaving}
                      className="h-7 text-xs"
                    >
                      {isSaving && <Loader2 className="h-3 w-3 mr-1.5 animate-spin" />}
                      Save
                    </Button>
                  </div>
                </>
              ) : (
                <div className="flex-1 flex items-center justify-center">
                  <p className="text-sm text-muted-foreground">
                    {environments.length === 0
                      ? 'Create an environment to get started.'
                      : 'Select an environment to edit.'}
                  </p>
                </div>
              )}
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* Delete confirmation */}
      <AlertDialog open={!!deleteCandidate} onOpenChange={(o) => !o && setDeleteCandidate(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete environment?</AlertDialogTitle>
            <AlertDialogDescription>
              <strong>{deleteCandidate}</strong> and all its variables will be permanently deleted.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDeleteEnv}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
```

**Step 2: Build to verify**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn build 2>&1 | tail -5
```

Expected: `✓ built`

**Step 3: Commit**

```bash
cd /home/numericlabs/data/rocket-api
git add frontend/src/components/collections/EnvironmentsDialog.tsx
git commit -m "feat(env): add EnvironmentsDialog component with CRUD and secret variable support"
```

---

### Task 6: Add env selector + Manage button to `RequestBuilder`

**Files:**
- Modify: `frontend/src/components/request-builder/RequestBuilder.tsx`

**Step 1: Add imports**

Find the top of the `RequestBuilder.tsx` import block. Add these imports (merge with existing import groups):

```tsx
import { Settings2 } from 'lucide-react'                          // add to lucide imports
import { EnvironmentsDialog } from '@/components/collections/EnvironmentsDialog'  // new import
```

Also add `useState` if not already imported (it is — keep as-is).

**Step 2: Add store references and dialog state**

In `RequestBuilder`, find:
```tsx
  // Collections store
  const { activeCollection } = useCollectionsStore()
```

Replace with:
```tsx
  // Collections store
  const { activeCollection, environments, activeEnvironment, setActiveEnvironment } = useCollectionsStore()
  const [envDialogOpen, setEnvDialogOpen] = useState(false)
```

**Step 3: Add the env selector and Manage button to the URL bar**

Find the name input at the top of the URL bar section:
```tsx
        <div className="px-4 pt-2 pb-4 border-b border-border bg-muted/40 shadow-sm space-y-2">
          <Input
            value={name}
            onChange={(e) => {
              setName(e.target.value)
              updateActiveName(e.target.value)
            }}
            placeholder="Untitled Request"
            className="h-7 text-sm font-medium border-0 border-b border-transparent hover:border-border focus:border-primary rounded-none bg-transparent px-0 focus-visible:ring-0 shadow-none"
          />
```

Replace with:
```tsx
        <div className="px-4 pt-2 pb-4 border-b border-border bg-muted/40 shadow-sm space-y-2">
          <div className="flex items-center gap-2">
            <Input
              value={name}
              onChange={(e) => {
                setName(e.target.value)
                updateActiveName(e.target.value)
              }}
              placeholder="Untitled Request"
              className="h-7 text-sm font-medium border-0 border-b border-transparent hover:border-border focus:border-primary rounded-none bg-transparent px-0 focus-visible:ring-0 shadow-none flex-1"
            />
            {/* Environment selector */}
            <div className="flex items-center gap-1 shrink-0">
              <Select
                value={activeEnvironment?.name ?? 'none'}
                onValueChange={(value) => {
                  if (value === 'none') {
                    setActiveEnvironment(null)
                  } else {
                    const env = environments.find(e => e.name === value) ?? null
                    setActiveEnvironment(env)
                  }
                }}
                disabled={!activeCollection}
              >
                <SelectTrigger className="h-7 text-xs w-[120px] gap-1 border-dashed">
                  <Globe className="h-3 w-3 text-muted-foreground shrink-0" />
                  <SelectValue placeholder="No Env" />
                </SelectTrigger>
                <SelectContent align="end">
                  <SelectItem value="none" className="text-xs">No Environment</SelectItem>
                  {environments.map(env => (
                    <SelectItem key={env.name} value={env.name} className="text-xs">{env.name}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={() => setEnvDialogOpen(true)}
                title="Manage environments"
                disabled={!activeCollection}
              >
                <Settings2 className="h-3.5 w-3.5" />
              </Button>
            </div>
          </div>
```

**Step 4: Add `Globe` to lucide imports**

Find the lucide-react import in `RequestBuilder.tsx` and add `Globe` to the list.

**Step 5: Render `EnvironmentsDialog` at the end of the component return**

Find the closing `</div>` of the component's root `<div ref={containerRef}>`. Before it, add:
```tsx
      <EnvironmentsDialog open={envDialogOpen} onOpenChange={setEnvDialogOpen} />
```

**Step 6: Build to verify**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn build 2>&1 | tail -5
```

Expected: `✓ built`. Fix any TS errors before continuing.

**Step 7: Commit**

```bash
cd /home/numericlabs/data/rocket-api
git add frontend/src/components/request-builder/RequestBuilder.tsx
git commit -m "feat(env): add env selector and Manage button to RequestBuilder URL bar"
```

---

### Task 7: Remove `EnvironmentsPanel` and clean up `App.tsx`

**Files:**
- Modify: `frontend/src/App.tsx`
- Delete: `frontend/src/components/collections/EnvironmentsPanel.tsx`

**Step 1: Remove panel from `App.tsx`**

In `frontend/src/App.tsx`, remove:
1. The import: `import { EnvironmentsPanel } from '@/components/collections/EnvironmentsPanel'`
2. The state: `const [showEnvironments, setShowEnvironments] = useState(false)`
3. The header button that toggles environments (the `<Button>` with text "Environments")
4. The right panel render: `{showEnvironments && (<EnvironmentsPanel />)}`

After removal, the `useState` import may still be needed (it is — used elsewhere). Verify.

**Step 2: Delete `EnvironmentsPanel.tsx`**

```bash
rm /home/numericlabs/data/rocket-api/frontend/src/components/collections/EnvironmentsPanel.tsx
```

**Step 3: Build to verify**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn build 2>&1 | tail -5
```

Expected: `✓ built`

**Step 4: Commit**

```bash
cd /home/numericlabs/data/rocket-api
git add frontend/src/App.tsx
git rm frontend/src/components/collections/EnvironmentsPanel.tsx
git commit -m "feat(env): remove EnvironmentsPanel and panel toggle — replaced by EnvironmentsDialog"
```

---

### Task 8: Manual smoke test

Start the development servers:

```bash
# Terminal 1
cd /home/numericlabs/data/rocket-api/backend && go run cmd/server/main.go

# Terminal 2
cd /home/numericlabs/data/rocket-api/frontend && yarn dev
```

Open `http://localhost:5173` and verify:

1. **No right panel** — the "Environments" button in the header is gone.
2. **Env selector visible** — in the request builder URL bar, the globe icon + "No Env" dropdown appears when a collection is active.
3. **Manage button** — clicking `⚙` opens the `EnvironmentsDialog`.
4. **Create environment** — type a name in the left column input, press Enter or click Add. Env appears in list. Backend creates `name.env`.
5. **Add variables** — select the new env, click "Add Variable", enter key/value, click Save. Variables appear on reload.
6. **Secret variables** — toggle the lock icon on a variable. Value becomes masked (dots). Toggle the eye to reveal. On Save, secret vars go to `name.env.secret`.
7. **Delete variable** — hover a row, click ×. Click Save to confirm.
8. **Delete environment** — hover env in left list, click trash icon. Confirm. Env disappears and files are removed from disk.
9. **Active env selector** — change env from the URL bar dropdown. Active env persists on page reload (localStorage).
10. **Variable substitution** — set `baseUrl=http://localhost:3000`, use `{{baseUrl}}/users` as the URL, send the request. Substituted URL is used.

---

### Task 9: Commit all uncommitted working-directory changes

There are currently uncommitted changes (the collection overview feature and related fixes). Commit them first before pushing.

```bash
cd /home/numericlabs/data/rocket-api
git add frontend/src/App.tsx \
        frontend/src/components/collections/CollectionsSidebar.tsx \
        frontend/src/components/collections/CollectionOverview.tsx \
        frontend/src/components/request-builder/RequestBuilder.tsx \
        frontend/src/components/request-builder/RequestTabs.tsx \
        frontend/src/lib/constants.ts \
        frontend/src/lib/collection-stats.ts \
        frontend/src/store/tabs-store.ts
git commit -m "feat(collection): add overview page with stats and method colors"
```

> **Note:** Do this task **before** Task 1, or at least before pushing. The working tree must be clean before the environments work starts, to avoid conflicts.
