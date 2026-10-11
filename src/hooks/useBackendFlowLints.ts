import { useEffect, useRef, useState } from 'react';
import type { FlowIssue } from '@/lib/flow-issues';
import { LINT_DEBOUNCE_MS, toFlowIssue } from '@/lib/flow-lint';
import { type Flow, type FlowLintEnvironment, lintFlow } from '@/lib/tauri-api';

export interface BackendLintOptions {
  /** The environments a run would use. A change lints again. */
  environment?: FlowLintEnvironment;
  /** Change this to lint again without an edit, for example after a save. */
  refreshKey?: number;
}

/**
 * Backend lint issues for the graph the canvas holds now. Lints at once
 * when a flow opens and after each pause in editing. It lints again when the
 * environment names or `refreshKey` change, because a saved request or a
 * variable may have changed outside the graph. A failed call shows no
 * backend issues; lints never block Run or Save.
 */
export function useBackendFlowLints(
  collection: string | null,
  flow: Flow | null,
  delayMs: number = LINT_DEBOUNCE_MS,
  options: BackendLintOptions = {},
): FlowIssue[] {
  const environmentName = options.environment?.environmentName ?? null;
  const globalEnvName = options.environment?.globalEnvName ?? null;
  const refreshKey = options.refreshKey ?? 0;
  const [issues, setIssues] = useState<FlowIssue[]>([]);
  // Bumped by every cleanup, so a response from an older effect is ignored.
  const generationRef = useRef(0);
  // Read when the timer fires, so the effect depends on the graph's content only.
  const flowRef = useRef(flow);
  flowRef.current = flow;
  // Node positions stay out of the key, so dragging a node does not lint.
  const key =
    collection && flow
      ? JSON.stringify({
          ...flow,
          nodes: flow.nodes.map((node) => ({ id: node.id, kind: node.kind })),
          refreshKey,
        })
      : null;
  // The flow whose lint ran last, so the first lint of a newly opened flow skips the delay.
  const lintedFlowRef = useRef<string | null>(null);
  const openKey = collection && flow ? `${collection}|${flow.name}` : null;

  useEffect(() => {
    const generation = generationRef.current;
    if (!collection || key === null) {
      setIssues([]);
      return;
    }
    const wait = lintedFlowRef.current === openKey ? delayMs : 0;
    lintedFlowRef.current = openKey;
    const timer = setTimeout(() => {
      const current = flowRef.current;
      if (!current) return;
      lintFlow(collection, current, { environmentName, globalEnvName })
        .then((lints) => {
          if (generationRef.current === generation) setIssues(lints.map(toFlowIssue));
        })
        .catch((err: unknown) => {
          if (generationRef.current !== generation) return;
          console.warn('[flow-lint] lint_flow failed', err);
          setIssues([]);
        });
    }, wait);
    return () => {
      clearTimeout(timer);
      generationRef.current += 1;
    };
  }, [collection, key, openKey, delayMs, environmentName, globalEnvName]);

  return issues;
}
