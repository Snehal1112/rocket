// Deliberate Hard Rule violation for the claude-review.yml smoke test.
// Raw <button> instead of shadcn/ui Button — delete this file after the
// smoke test in docs/superpowers/plans/2026-09-25-claude-pr-review.md
// Task 2 is complete.
export function SmokeTestButton() {
  return <button onClick={() => {}}>Click me</button>
}
