# Factory Live Token Bar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the token bar on a factory dashboard card follow the running agent, instead of moving only when an agent run ends.

**Architecture:** The driver writes the board `tokens` field once per finished agent run, but every tool event in `events.jsonl` carries a running weighted total for the job. The dashboard server already attaches those events to each job as agents (`AgentState.tokens_weighted`). A new `live_token_view` in `state.rs` takes the higher of the board total and the highest agent total, and marks the view `live` when the events are ahead. The card keeps its look and only changes its tooltip text when the view is live. The dashboard stays read-only and the driver is not touched.

**Tech Stack:** Rust (`tools/factory-server`, its own cargo workspace), plain JavaScript (`tools/factory-server/static/board.js`), `node --test`.

**Spec:** none. This is a bounded change approved in chat on 2026-10-17: the card bar should use the live event total when it is ahead of the board.

## Global Constraints

- Always `cargo ... -j4`. Run cargo from `tools/factory-server/` with `cargo test -j4` (this crate is its own workspace). Never `--workspace`, never set `CARGO_TARGET_DIR`.
- No panicking unwrap or expect calls in production Rust; tests may use them.
- The dashboard never writes the board, the events log or any driver file.
- Board JSON and `/api/state` stay backward compatible: `token_view` keeps its existing fields. The new field `live` is a plain bool.
- No inline styles (the CSP forbids them). Bar widths still go through `data-pct`.
- Comments are short full sentences that end with a punctuation mark. No emojis.
- Commit through the `dev-workflow-skills:1-git-commit` skill with explicit paths (no `git add -A` or `.`).

## Review Focus

- A finished job (merged, needs-human, cancelled): the board total and the last event total are equal, so the bar must not change. Pinned by a test where the live total equals the board total.
- A job with no board tokens and no agent events keeps `token_view` as `None` (no bar), as the existing tests assert.
- The live total never lowers the bar: when the board total is higher than every agent total (for example a message or KB agent that started its count at 0), the board value wins. Pinned by a test.
- Fraction stays clamped between 0 and 1, and a zero budget gives 0.
- `raw` is unknown from events: a live view keeps the board raw (or 0), and the tooltip must not claim a raw figure when live.

---

### Task 1: Live token view in the server and the card tooltip

**Files:**
- Modify: `tools/factory-server/src/state.rs`
- Modify: `tools/factory-server/static/board.js`
- Modify: `tools/factory-server/tests/board.test.js`
- Modify: `tools/factory-server/README.md` and `.claude/software-factory.md` (one sentence each, only where the token bar is described)

**Interfaces:**
- Produces: `TokenView.live: bool`; `pub fn live_token_view(job: &Job, agents: &[AgentState]) -> Option<TokenView>` in `state.rs`; `build_state` sets each `JobView.token_view` from it after the agents are attached.

- [ ] **Step 1: Write the failing Rust tests**

In the `tests` module of `tools/factory-server/src/state.rs`, near the existing `the_token_view_has_a_fraction_between_zero_and_one` test (use its `job(...)` helper and the `AgentState` literal used by the nearby tests), add:

```rust
    fn agent_with(tokens_weighted: u64) -> AgentState {
        // Copy the AgentState literal used by the other tests in this module and set the tokens.
        let mut agent = sample_agent();
        agent.tokens_weighted = tokens_weighted;
        agent
    }

    #[test]
    fn a_running_agent_moves_the_bar_ahead_of_the_board() {
        let mut item = job("001", "build", 1.0);
        item.tokens = Some(Tokens { weighted: 100_000, raw: 800_000 });
        let view = live_token_view(&item, &[agent_with(250_000)]).expect("view");
        assert_eq!(view.weighted, 250_000);
        assert_eq!(view.raw, 800_000);
        assert!(view.live);
        assert!((view.fraction - 250_000.0 / DEFAULT_BUDGET_WEIGHTED as f64).abs() < 1e-9);
    }

    #[test]
    fn a_job_without_board_tokens_gets_a_live_bar_from_its_agent() {
        let item = job("001", "build", 1.0);
        let view = live_token_view(&item, &[agent_with(60_000)]).expect("view");
        assert_eq!((view.weighted, view.raw, view.live), (60_000, 0, true));
    }

    #[test]
    fn a_finished_job_keeps_the_board_figure() {
        let mut item = job("001", "merged", 1.0);
        item.tokens = Some(Tokens { weighted: 300_000, raw: 2_000_000 });
        let view = live_token_view(&item, &[agent_with(300_000), agent_with(40_000)]).expect("view");
        assert_eq!((view.weighted, view.raw, view.live), (300_000, 2_000_000, false));
    }

    #[test]
    fn the_board_wins_when_every_agent_total_is_lower() {
        let mut item = job("001", "build", 1.0);
        item.tokens = Some(Tokens { weighted: 90_000, raw: 500_000 });
        let view = live_token_view(&item, &[agent_with(20_000)]).expect("view");
        assert_eq!((view.weighted, view.live), (90_000, false));
    }

    #[test]
    fn no_board_tokens_and_no_agent_tokens_means_no_view() {
        let item = job("001", "build", 1.0);
        assert!(live_token_view(&item, &[]).is_none());
        assert!(live_token_view(&item, &[agent_with(0)]).is_none());
    }

    #[test]
    fn the_live_fraction_is_clamped_and_a_zero_budget_gives_zero() {
        let mut item = job("001", "build", 1.0);
        item.budget_weighted = Some(1_000);
        let view = live_token_view(&item, &[agent_with(5_000)]).expect("view");
        assert_eq!(view.fraction, 1.0);
        item.budget_weighted = Some(0);
        let view = live_token_view(&item, &[agent_with(5_000)]).expect("view");
        assert_eq!(view.fraction, 0.0);
    }
```

`sample_agent()` stands for the helper or literal the module already uses to build an `AgentState` (around line 414 the module builds one with `tokens_weighted: 0`); if no helper exists, add a small private `fn sample_agent() -> AgentState` that returns that literal.

- [ ] **Step 2: Run them to see them fail**

Run: `cd tools/factory-server && cargo test -j4 state::tests::a_running`
Expected: compile errors (no `live_token_view`, no `live` field).

- [ ] **Step 3: Implement**

In `state.rs`:

1. Add `pub live: bool,` to `TokenView` with the doc comment `/// True when the total comes from a running agent's events and is ahead of the board.` Set `live: false` in `token_view`.
2. Add after `token_view`:

```rust
/// The card bar figures: the board total, or the highest running total from the job's agent events
/// when that is ahead. The board is written only when an agent run ends, so a running agent is ahead of it.
pub fn live_token_view(job: &Job, agents: &[AgentState]) -> Option<TokenView> {
    let board = token_view(job);
    let live = agents.iter().map(|a| a.tokens_weighted).max().unwrap_or(0);
    if live == 0 || live <= board.as_ref().map_or(0, |v| v.weighted) {
        return board;
    }
    let budget = job.budget_weighted.unwrap_or(DEFAULT_BUDGET_WEIGHTED);
    let fraction = if budget == 0 { 0.0 } else { (live as f64 / budget as f64).clamp(0.0, 1.0) };
    Some(TokenView { weighted: live, raw: board.map_or(0, |v| v.raw), budget, fraction, live: true })
}
```

3. In `build_state`, in the loop that sorts each job's agents (`for view in &mut jobs { view.agents.sort_by(...) }`), add after the sort: `view.token_view = live_token_view(&view.job, &view.agents);`.
4. Fix any other `TokenView { .. }` literal in the crate (grep `TokenView {`) by adding `live: false`.

- [ ] **Step 4: Write the failing JS test**

In `tools/factory-server/tests/board.test.js`, next to the existing `tokensBar` tests, add:

```js
test("tokensBar tooltip says live and drops the raw figure when the view is live", () => {
  const html = board.tokensBar(makeJob({ token_view: { weighted: 250000, raw: 0, budget: 3000000, fraction: 0.083, live: true } }));
  assert.ok(html.includes("250K"), html);
  assert.ok(html.includes("including the agent that is running"), html);
  assert.ok(!html.includes("Raw:"), html);
  const settled = board.tokensBar(makeJob({ token_view: { weighted: 250000, raw: 1900000, budget: 3000000, fraction: 0.083, live: false } }));
  assert.ok(settled.includes("Raw: 1.9M"), settled);
  assert.ok(!settled.includes("running"), settled);
});
```

Run: `cd tools/factory-server && node --test tests/board.test.js`. Expected: the new test fails.

- [ ] **Step 5: Implement the tooltip**

In `board.js` `tokensBar`, replace the `title="..."` attribute of the `.tokens` div so it is built from a variable:

```js
    const title = view.live
      ? "Weighted tokens so far, including the agent that is running, against the job budget."
      : `Weighted tokens against the job budget. Raw: ${formatTokens(view.raw)}`;
```

and use `title="${escapeHtml(title)}"` in the template. Nothing else in the markup changes.

- [ ] **Step 6: Docs**

In `tools/factory-server/README.md` and in the dashboard section of `.claude/software-factory.md`, wherever the token bar is described, add one sentence: the bar shows the board total, or the running total from the agent events when that is higher, so it moves while an agent works.

- [ ] **Step 7: Run everything**

Run:
- `cd tools/factory-server && cargo test -j4`
- `cd tools/factory-server && node --test tests/board.test.js tests/patch.test.js`
- `bash tools/factory-server/tests/smoke.sh` (from the repository root)

Expected: all PASS. Report the pass counts.

Next plan: none
