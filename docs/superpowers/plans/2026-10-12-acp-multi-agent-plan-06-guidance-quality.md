# ACP Multi-Agent Support, Plan 6: Guidance Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the assistant good at guiding and correcting the user on an endpoint: it can read a request's docs, it tells the user clearly what is wrong and how to fix it, it knows how to write good docs, and the user can review a docs proposal as rendered Markdown.

**Architecture:** Three small changes on top of Plans 4 and 5. The `get_request` MCP tool returns the request's `docs` (today only folder docs are readable). The proposal card shows a docs change as a Markdown diff and preview instead of a JSON-escaped string. The assistant instructions and the slash commands teach the guide-and-correct workflow and say plainly what is needed to try an endpoint live.

**Tech Stack:** Rust (rocket-app, src-tauri), React + TypeScript, Monaco diff editor (already used), Vitest, Biome.

**Spec:** `docs/features/acp-multi-agent-support.md`; findings in `docs/superpowers/plans/2026-10-12-acp-multi-agent-index.md` (section "Reasoning findings"). Independent of Plans 4 and 5 in code, but run after them.

## Global Constraints

- Run cargo with `-j4`. Never `cargo test --workspace` or `--all`.
- Never call unwrap in production paths (a project hook blocks it in Rust edits). `expect("...")` is fine in tests.
- UI uses shadcn/ui primitives only, lucide-react icons only. Multi-line editors are Monaco only (the proposal diff already is).
- Never `git add -A`, `git add .` or `--all`. Explicit paths. Commit through the `dev-workflow-skills:1-git-commit` skill (conventional commits).
- Security parity: `get_request` returns docs the same way `get_folder_settings` returns folder docs: as plain text, untruncated, no extra masking. Scripts and folder docs are already returned this way. Do not add a new unmasked channel for anything else.
- The agent still cannot run a request unless Agent mode, the collection's run switch and the local trust confirmation are all on. This plan does not weaken that. It only makes the assistant explain it.
- Ruling: stored conversation messages stay capped at 16 KiB each (`MAX_MESSAGE_TEXT_BYTES`). A docs proposal is applied from the proposal card, not from the stored chat text, so the cap only shortens the saved copy of a long chat message. Raising it would grow every stored conversation, so it is not changed.
- No `OpenCollection` read needed: docs are read from the already-parsed request, and no YAML is written here.

## Review Focus

- A request with no docs must return `docs: null`, and a request with typed docs (`content` plus `type`) must return its text (Task 15 tests).
- Reading docs must not change what the agent can do to a request: `get_request` stays read-only and works in Ask mode (Task 15 keeps the existing mode tests green).
- A docs-only proposal must show the docs diff and no empty JSON diff (Task 16 tests).
- A proposal with a very long docs text must still render: the preview scrolls and the diff editor is the existing Monaco one (Task 16 manual check).
- The assistant must never be told that it can run an endpoint when it cannot: the instructions must name all three conditions (Task 17 test).

---

### Task 15: `get_request` returns the request's docs

**Files:**
- Modify: `crates/rocket-app/src/mcp_read_views.rs` (`MaskedRequest` at line 145-160, `from_request` at line 163-183, tests)
- Modify: `src-tauri/src/mcp/tool_server.rs` (the `get_request` tool description at line 935, tests)
- Test: `crates/rocket-app/src/mcp_read_views.rs`, `src-tauri/src/mcp/tool_server.rs`

**Interfaces:**
- Consumes: `Request.docs: Option<Documentation>` and `Description::content()` (`rocket-shared`).
- Produces: `MaskedRequest.docs: Option<String>` (serialized as `docs`, `null` when the request has none).

- [ ] **Step 1: Write the failing tests**

Add inside `tests` of `crates/rocket-app/src/mcp_read_views.rs`, after `a_request_view_masks_headers_query_auth_and_url_but_keeps_scripts_whole`:

```rust
    #[test]
    fn a_request_view_carries_the_docs_text_in_every_documentation_shape() {
        let mut request = Request::new("Login", HttpMethod::Post, "https://api.test/login");

        request.docs = Some(rocket_shared::Documentation::text("# Login\nReturns a token."));
        let view = MaskedRequest::from_request("auth/login.yml", &request);
        assert_eq!(view.docs.as_deref(), Some("# Login\nReturns a token."));

        request.docs = Some(rocket_shared::Documentation::typed(
            "Typed *docs*",
            "text/markdown",
        ));
        let view = MaskedRequest::from_request("auth/login.yml", &request);
        assert_eq!(view.docs.as_deref(), Some("Typed *docs*"));

        request.docs = None;
        let view = MaskedRequest::from_request("auth/login.yml", &request);
        assert_eq!(view.docs, None);
        let json = serde_json::to_value(&view).expect("json");
        assert!(json["docs"].is_null(), "{json}");
    }
```

Add inside `tests` of `src-tauri/src/mcp/tool_server.rs` next to `get_info_reports_the_rocket_tool_server_identity`:

```rust
    #[test]
    fn the_get_request_description_says_it_returns_the_docs() {
        let server: RocketMcpToolServer<tauri::test::MockRuntime> =
            RocketMcpToolServer::new(test_app_handle(), "session-1".to_string());
        let tool = server
            .tool_router
            .list_all()
            .into_iter()
            .find(|tool| tool.name == "get_request")
            .expect("get_request is registered");
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(description.contains("docs"), "{description}");
        assert!(description.contains("masked"), "it keeps its masking note: {description}");
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p rocket-app -j4 request_view_carries_the_docs`
Expected: FAIL to compile (`no field docs on MaskedRequest`).

- [ ] **Step 3: Implement**

In `mcp_read_views.rs` add to `MaskedRequest` after `tests`:

```rust
    /// The request's documentation text, as stored. Typed docs lose their
    /// content type here: the agent only needs the text.
    pub docs: Option<String>,
```

and in `from_request` add after `tests: request.tests.clone(),`:

```rust
            docs: request
                .docs
                .as_ref()
                .and_then(|docs| docs.content())
                .map(str::to_string),
```

Update the doc comment of `MaskedRequest` to "The full definition of one HTTP request, with credentials masked. Scripts and docs are returned in full."

In `tool_server.rs` change the `get_request` description to:

```rust
        description = "Read one request's full definition (URL, headers, params, body, auth, scripts and docs). Literal credentials are masked; {{variable}} references are kept."
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p rocket-app -j4 mcp_read_views && cargo test -p rocket-app -j4 mcp_tool_service && cargo test -p rocket -j4 --lib mcp::tool_server`
Expected: PASS (the new tests and every existing read-view and tool test).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/mcp_read_views.rs src-tauri/src/mcp/tool_server.rs
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(assistant): let the agent read a request's docs`.

---

### Task 16: Review a docs proposal as Markdown

**Files:**
- Modify: `src/lib/assistant/proposal-view.ts` (`ProposalPreview`, `ProposalDiff`, `proposalPreview`, `loadProposalDiff`)
- Create: `src/components/assistant/ProposalDocsPreview.tsx`
- Modify: `src/components/assistant/AssistantProposalCard.tsx` (the `diffContent` and `textPreview` blocks)
- Test: `src/lib/assistant/__tests__/proposal-view.test.ts`, `src/components/assistant/__tests__/ProposalDocsPreview.test.tsx`

**Interfaces:**
- Consumes: `AgentProposal` change shapes (`updateRequest`, `updateFolderSettings`, `createRequest`) and `getRequest` / `getFolderSettings`.
- Produces:
  - `ProposalDiff.docs?: { before: string; after: string }` (present only when the patch changes docs).
  - `ProposalPreview` definition variant: `{ kind: 'definition'; text: string; docs?: string }`.
  - `ProposalDocsPreview({ markdown })` component.

- [ ] **Step 1: Write the failing view tests**

Add inside `describe('proposal-view', ...)` of `src/lib/assistant/__tests__/proposal-view.test.ts`:

```ts
  it('shows a docs change apart from the JSON diff of the other fields', async () => {
    vi.mocked(api.getRequest).mockResolvedValue(
      makeRequest({ url: 'https://api.test/orders/1', docs: '# Old\nline' }),
    );
    const proposal = makeProposal({
      change: {
        op: 'updateRequest',
        collection: 'orders',
        requestPath: 'get.yml',
        patch: { url: 'https://api.test/orders/2', docs: '# New\nline' },
      },
    });
    const diff = await loadProposalDiff(proposal);
    expect(diff?.docs).toEqual({ before: '# Old\nline', after: '# New\nline' });
    expect(JSON.parse(diff?.before ?? '{}')).toEqual({ url: 'https://api.test/orders/1' });
    expect(JSON.parse(diff?.after ?? '{}')).toEqual({ url: 'https://api.test/orders/2' });
  });

  it('gives a docs-only update an empty JSON part and the docs diff', async () => {
    vi.mocked(api.getRequest).mockResolvedValue(makeRequest({ docs: null }));
    const proposal = makeProposal({
      change: {
        op: 'updateRequest',
        collection: 'orders',
        requestPath: 'get.yml',
        patch: { docs: '# Orders\nLists orders.' },
      },
    });
    const diff = await loadProposalDiff(proposal);
    expect(diff?.docs).toEqual({ before: '', after: '# Orders\nLists orders.' });
    expect(diff?.before).toBe(diff?.after);
    expect(JSON.parse(diff?.before ?? 'x')).toEqual({});
  });

  it('shows folder docs apart from the other folder settings', async () => {
    vi.mocked(api.getFolderSettings).mockResolvedValue({
      headers: [],
      variables: [],
      docs: 'old folder docs',
    } as unknown as Awaited<ReturnType<typeof api.getFolderSettings>>);
    const proposal = makeProposal({
      change: {
        op: 'updateFolderSettings',
        collection: 'orders',
        folderPath: 'v2',
        patch: { docs: 'new folder docs' },
      },
    });
    const diff = await loadProposalDiff(proposal);
    expect(diff?.docs).toEqual({ before: 'old folder docs', after: 'new folder docs' });
  });

  it('leaves a diff without a docs change unchanged', async () => {
    vi.mocked(api.getRequest).mockResolvedValue(makeRequest({ url: 'https://old.test' }));
    const proposal = makeProposal({
      change: {
        op: 'updateRequest',
        collection: 'orders',
        requestPath: 'get.yml',
        patch: { url: 'https://new.test' },
      },
    });
    const diff = await loadProposalDiff(proposal);
    expect(diff?.docs).toBeUndefined();
  });

  it('previews the docs of a new request as Markdown and keeps them out of the JSON', () => {
    const proposal = makeProposal({
      change: {
        op: 'createRequest',
        collection: 'orders',
        folderPath: '',
        request: {
          name: 'List orders',
          method: 'GET',
          url: 'https://api.test/orders',
          docs: '# Orders\nLists orders.',
        },
      },
    });
    const preview = proposalPreview(proposal.change);
    expect(preview.kind).toBe('definition');
    if (preview.kind !== 'definition') return;
    expect(preview.docs).toBe('# Orders\nLists orders.');
    expect(preview.text).not.toContain('Lists orders');
    expect(JSON.parse(preview.text)).toMatchObject({ name: 'List orders' });
  });
```

If the compiler reports a missing required field on the `request` object of the last test, add it (copy the value shape from `AgentProposedRequest` in `src/lib/tauri-api.ts`).

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test proposal-view`
Expected: FAIL (`diff.docs` is undefined).

- [ ] **Step 3: Implement the view logic**

In `src/lib/assistant/proposal-view.ts`:

1. Change the preview type and the diff type:

```ts
export type ProposalPreview =
  | { kind: 'diff' }
  | { kind: 'definition'; text: string; docs?: string }
  | { kind: 'line'; text: string };

export interface ProposalDiff {
  before: string;
  after: string;
  language: string;
  /** The docs change, shown as Markdown apart from the JSON. Present only when docs change. */
  docs?: { before: string; after: string };
}
```

2. Add a helper below `stringifyPicked`:

```ts
// The text of a docs value. A missing or non-text value counts as empty.
function docsText(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

// The keys of a patch that go into the JSON diff, and the docs change when there is one.
function splitDocs(
  patch: Record<string, unknown>,
  currentDocs: unknown,
): { keys: string[]; docs?: { before: string; after: string } } {
  const keys = Object.keys(patch).filter((key) => patch[key] !== undefined && key !== 'docs');
  if (typeof patch.docs !== 'string') return { keys };
  return { keys, docs: { before: docsText(currentDocs), after: patch.docs } };
}
```

3. In `proposalPreview` replace the `createRequest` case with:

```ts
    case 'createRequest': {
      const { docs, ...rest } = change.request;
      return {
        kind: 'definition',
        text: JSON.stringify(rest, null, 2),
        ...(typeof docs === 'string' && docs !== '' ? { docs } : {}),
      };
    }
```

4. In `loadProposalDiff`, in the `updateFolderSettings` branch replace `const keys = Object.keys(patch).filter((key) => patch[key] !== undefined);` with `const { keys, docs } = splitDocs(patch, current.docs);` and change the returned object to:

```ts
    return {
      before: stringifyPicked(before, keys),
      after: stringifyPicked(patch, keys),
      language: 'json',
      ...(docs ? { docs } : {}),
    };
```

   In the final request-update part replace `const keys = Object.keys(patch).filter((key) => patch[key] !== undefined);` with `const { keys, docs } = splitDocs(patch, current.docs);` and the returned object with:

```ts
  return {
    before: stringifyPicked(asRecord(current), keys),
    after: stringifyPicked(patch, keys),
    language,
    ...(docs ? { docs } : {}),
  };
```

- [ ] **Step 4: Run the view tests**

Run: `yarn test proposal-view`
Expected: PASS (the five new tests and every existing one: a script edit diff and a patch without docs return the same objects as before).

- [ ] **Step 5: Write the failing component test**

Create `src/components/assistant/__tests__/ProposalDocsPreview.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ProposalDocsPreview } from '@/components/assistant/ProposalDocsPreview';

describe('ProposalDocsPreview', () => {
  it('renders the Markdown of the proposed docs', () => {
    render(<ProposalDocsPreview markdown={'# Orders\n\n- first item'} />);
    expect(screen.getByText('Docs preview')).toBeInTheDocument();
    expect(screen.getByText('Orders')).toBeInTheDocument();
    expect(screen.getByText('first item')).toBeInTheDocument();
  });

  it('shows plain text for docs without Markdown', () => {
    render(<ProposalDocsPreview markdown='Returns the order list.' />);
    expect(screen.getByText('Returns the order list.')).toBeInTheDocument();
  });
});
```

- [ ] **Step 6: Run it to verify it fails**

Run: `yarn test ProposalDocsPreview`
Expected: FAIL (module not found).

- [ ] **Step 7: Implement the component and use it in the card**

Create `src/components/assistant/ProposalDocsPreview.tsx`:

```tsx
import { MarkdownRenderer } from '@/components/collections/MarkdownRenderer';

/** The proposed docs rendered as Markdown. It scrolls when the docs are long. */
export function ProposalDocsPreview({ markdown }: { markdown: string }) {
  return (
    <div className='max-h-56 overflow-auto rounded-md border p-2 text-sm'>
      <p className='mb-1 text-xs font-semibold text-muted-foreground'>Docs preview</p>
      <MarkdownRenderer restricted>{markdown}</MarkdownRenderer>
    </div>
  );
}
```

In `AssistantProposalCard.tsx`:

1. Add `import { ProposalDocsPreview } from './ProposalDocsPreview';` next to the other relative imports (below the `lazy` import block, or with the other component imports).
2. Replace the first branch of `diffContent` (`{diff ? (<Suspense ...>...</Suspense>) : diffFailed ? ...`) so that the `diff ?` branch reads:

```tsx
      {diff ? (
        <div className='space-y-2'>
          {diff.before !== diff.after && (
            <Suspense fallback={<EditorSkeleton />}>
              <ProposalDiffEditor
                original={diff.before}
                modified={diff.after}
                language={diff.language}
              />
            </Suspense>
          )}
          {diff.docs && (
            <div className='space-y-2'>
              <p className='text-xs font-semibold text-muted-foreground'>Docs change</p>
              <Suspense fallback={<EditorSkeleton />}>
                <ProposalDiffEditor
                  original={diff.docs.before}
                  modified={diff.docs.after}
                  language='markdown'
                />
              </Suspense>
              <ProposalDocsPreview markdown={diff.docs.after} />
            </div>
          )}
        </div>
      ) : diffFailed ? (
```

   (keep the rest of the ternary, from `diffFailed ? (` onward, unchanged).
3. In `textPreview` extend the `definition` branch:

```tsx
      {preview.kind === 'definition' && (
        <div className='space-y-2'>
          <pre className='max-h-56 overflow-auto whitespace-pre-wrap break-all rounded-md bg-muted p-2 font-mono text-xs'>
            {preview.text}
          </pre>
          {preview.docs && <ProposalDocsPreview markdown={preview.docs} />}
        </div>
      )}
```

- [ ] **Step 8: Run all checks for this task**

Run: `yarn test proposal-view ProposalDocsPreview AssistantProposalCard && yarn tsc --noEmit && yarn check`
Expected: PASS. If an existing `AssistantProposalCard` test snapshots the definition preview, update it to the new wrapper `div`.

- [ ] **Step 9: Manual check in the app**

Run `yarn tauri dev`, start an assistant in Edit mode on a request with existing docs and ask `Rewrite this request's docs with the sections Purpose, Parameters and Example.` Expected: the proposal card, expanded, shows a "Docs change" Markdown diff (old docs on the left side of the change, new docs added) and a "Docs preview" below it rendered as Markdown, and no `\n` escapes. A docs-only proposal shows no JSON diff. Accept it and confirm the request's Docs tab shows the new text.

- [ ] **Step 10: Commit**

```bash
git add src/lib/assistant/proposal-view.ts src/components/assistant/ProposalDocsPreview.tsx \
  src/components/assistant/AssistantProposalCard.tsx \
  src/lib/assistant/__tests__/proposal-view.test.ts \
  src/components/assistant/__tests__/ProposalDocsPreview.test.tsx
```

Add `src/components/assistant/__tests__/AssistantProposalCard.test.tsx` if you edited it. Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(assistant): review docs proposals as Markdown`.

---

### Task 17: Teach the assistant to guide and correct, and say what live testing needs

**Files:**
- Modify: `crates/rocket-app/src/mcp_tool_service.rs` (the end of `WORKSPACE_ASSISTANT_INSTRUCTIONS` at line 110-121, tests)
- Modify: `src/lib/assistant/slash-commands.ts`
- Modify: `docs/features/assistant-composer.md` (the line that lists the slash commands, if any)
- Modify: `docs/features/assistant-followups.md`
- Test: `crates/rocket-app/src/mcp_tool_service.rs`, `src/lib/assistant/__tests__/slash-commands.test.ts`

**Interfaces:**
- Consumes: the tools `get_request` (now with docs), `get_folder_settings`, `get_collection_settings`, `get_history`, `propose_changes`, `run_request` (existing).
- Produces: a longer `WORKSPACE_ASSISTANT_INSTRUCTIONS`; slash commands `review` and `howto`; a rewritten `doc` template.

- [ ] **Step 1: Write the failing instruction test**

Add inside `tests` of `crates/rocket-app/src/mcp_tool_service.rs`:

```rust
    #[test]
    fn the_instructions_teach_guiding_and_name_everything_live_testing_needs() {
        for phrase in [
            "get_request (it includes the request's docs)",
            "folder settings above it",
            "Think the problem through",
            "propose the smallest change",
            "Not documented",
            "attaches the last response",
            "Agent mode",
            "run switch",
            "confirmed on this computer",
            "do not retry",
        ] {
            assert!(
                WORKSPACE_ASSISTANT_INSTRUCTIONS.contains(phrase),
                "missing: {phrase}"
            );
        }
        // The first-prompt text stays within a sensible size.
        assert!(WORKSPACE_ASSISTANT_INSTRUCTIONS.len() < 4096);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p rocket-app -j4 the_instructions_teach_guiding`
Expected: FAIL (`missing: get_request (it includes the request's docs)`).

- [ ] **Step 3: Extend the instructions**

In `mcp_tool_service.rs` replace the last line of the `WORKSPACE_ASSISTANT_INSTRUCTIONS` literal, which is currently:

```rust
read them with get_folder_settings before advising on how a request behaves.";
```

with:

```rust
read them with get_folder_settings before advising on how a request behaves. \
How to help: when the user asks about an endpoint, first read it: the request with get_request \
(it includes the request's docs), the folder settings above it and the collection settings. \
Compare what the request sends with its docs and its tests, say plainly what is wrong or missing \
and why, and check auth, headers, path and query parameters, body shape and variables. Think the \
problem through before you propose a fix. When you change something, propose the smallest change \
that fixes it with propose_changes (Edit mode) and explain it in one or two sentences. When you \
write documentation, use Markdown with the sections that apply: Purpose, Authentication, \
Parameters, Request body, Responses and errors, Example. Base every statement on fields you \
read, and write Not documented where you do not know. You see a live response only if the user \
attaches the last response with # or lets you run the request. Running needs Agent mode, the \
collection's run switch turned on, and the run permission confirmed on this computer. When a \
tool refuses, tell the user exactly which of these to turn on and do not retry.";
```

Update the doc comment above the constant if it says the text covers only reading and proposing.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p rocket-app -j4 mcp_tool_service && cargo test -p rocket -j4 --test acp_mcp_start_workspace_assistant`
Expected: PASS (the integration test compares the sent `_meta` with the constant itself, so it follows the new text).

- [ ] **Step 5: Write the failing slash command tests**

Replace the first two tests of `src/lib/assistant/__tests__/slash-commands.test.ts` with:

```ts
  it('offers the seven Rocket templates in order', () => {
    expect(filterSlashCommands('').map((c) => c.name)).toEqual([
      'explain',
      'tests',
      'fix',
      'scaffold',
      'doc',
      'review',
      'howto',
    ]);
  });

  it('filters by name prefix, ignoring case', () => {
    expect(filterSlashCommands('te').map((c) => c.name)).toEqual(['tests']);
    expect(filterSlashCommands('EX').map((c) => c.name)).toEqual(['explain']);
    expect(filterSlashCommands('re').map((c) => c.name)).toEqual(['review']);
    expect(filterSlashCommands('ho').map((c) => c.name)).toEqual(['howto']);
    expect(filterSlashCommands('zzz')).toEqual([]);
  });

  it('asks for documentation with the standard sections and only what the request shows', () => {
    const doc = SLASH_COMMANDS.find((c) => c.name === 'doc');
    expect(doc?.template).toContain('Purpose');
    expect(doc?.template).toContain('Responses and errors');
    expect(doc?.template).toMatch(/only what/i);
  });
```

- [ ] **Step 6: Run them to verify they fail**

Run: `yarn test slash-commands`
Expected: FAIL (five commands, no `review`).

- [ ] **Step 7: Implement the slash commands**

In `src/lib/assistant/slash-commands.ts` replace the `doc` entry and add two entries after it:

```ts
  {
    name: 'doc',
    description: 'Write documentation for a request',
    template:
      'Write Markdown documentation for this request with the sections Purpose, Authentication, Parameters, Request body, Responses and errors, and Example. Use only what the request, its folder settings and its tests show, and write "Not documented" where you do not know: ',
  },
  {
    name: 'review',
    description: 'Review a request for mistakes and say how to fix them',
    template:
      'Review this request for mistakes. Check its URL, auth, headers, parameters, body, scripts and tests against its docs and its folder settings, explain what is wrong and why, and propose the smallest fixes: ',
  },
  {
    name: 'howto',
    description: 'Explain how to call an endpoint correctly',
    template:
      'Explain step by step how to call this endpoint correctly: which variables, auth and headers I need, and what to check if it fails: ',
  },
```

- [ ] **Step 8: Run the frontend checks**

Run: `yarn test slash-commands Composer && yarn tsc --noEmit && yarn check`
Expected: PASS. If a composer completion test lists the commands or counts them, update it to the seven names above.

- [ ] **Step 9: Update the docs**

Run `grep -n "slash\|/doc\|five" docs/features/assistant-composer.md`. In the line that lists the Rocket slash commands, add `/review` and `/howto` and change "five" to "seven". Append this section to `docs/features/assistant-followups.md`:

```markdown
## Reasoning and guidance (2026-10-12)

Plans 4 to 6 in `docs/superpowers/plans/2026-10-12-acp-multi-agent-index.md` cover thinking visibility, reasoning level and guidance. What is deliberately left as it is:

- Stored conversation messages stay capped at 16 KiB each. A docs proposal is applied from the proposal card, so only the saved chat copy is shortened.
- The agent still has no response history. `get_history` returns briefs only. A live response needs Agent mode, the collection's run switch and the local trust confirmation, or the user attaching the last response with `#`. The instructions now say so.
- `run_request` returns an 8 KB masked response body at most. Raising it needs a security review of the masking.
- Claude often sends empty thinking text on recent models, so the Reasoning block can stay empty for Claude. A summarized-thinking option in the adapter `_meta` is untested.
- Gemini returns its models in the older `models` field and no config options, so the composer shows no model picker for Gemini. The agent dialog's Gemini model field covers it.
- The model setting for Claude (the composer's Model picker) is still remembered per agent only.
```

- [ ] **Step 10: Manual check in the app**

Run `yarn tauri dev`, start the assistant in Edit mode on a request that has a wrong header or a missing variable, and type `/review `. Expected: the template appears; the assistant reads the request, its folder settings and its docs (Tool lines show `get_request`, `get_folder_settings`), explains the problem in plain words, and proposes a small fix as a proposal card. In Ask mode, ask `Run this request and tell me why it fails.` Expected: the assistant says live running needs Agent mode, the collection's run switch and the run permission on this computer, and does not retry.

- [ ] **Step 11: Commit**

```bash
git add crates/rocket-app/src/mcp_tool_service.rs src/lib/assistant/slash-commands.ts \
  src/lib/assistant/__tests__/slash-commands.test.ts docs/features/assistant-composer.md \
  docs/features/assistant-followups.md
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(assistant): teach the agent to guide and correct, add /review and /howto`.

---

## Next plan

**Next: the permission card plan** (`docs/superpowers/plans/2026-10-12-acp-permission-card.md`, 7 tasks, already written and not started). It is needed for `Generic` agents, whose permission requests Rocket still denies.

When Tasks 15 to 17 are committed with green checks, my Claude executes that plan next, in the same worktree, without asking again. Before starting, re-read its Task 1 and Task 3: it was written before `AgentKind`, `session_plan`, `AcpUpdate::Thought` and the tuning struct existed, so check that its `publish_update` edit and its `AcpSessionClient` trait change still apply, and fix the plan text first if they do not. Ledger: `.superpowers/sdd/2026-10-12-acp-permission-card/progress.md` (create it).

After that, one small plan per further agent (Qwen, Codex, OpenCode, Goose, Copilot, Cursor), each written when needed. See the index.
