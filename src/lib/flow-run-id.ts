/**
 * Returns a new id for one flow run. The toolbar sends it with run_flow and
 * matches every flow-run-* event by it, so two tabs of one flow never share
 * a run. It has a module of its own, so tests can choose the id.
 */
export function newFlowRunId(): string {
  return crypto.randomUUID();
}
