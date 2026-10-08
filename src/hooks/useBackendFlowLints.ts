import { useEffect, useRef, useState } from 'react';
import type { FlowIssue } from '@/lib/flow-issues';
import { LINT_DEBOUNCE_MS, toFlowIssue } from '@/lib/flow-lint';
import { type Flow, lintFlow } from '@/lib/tauri-api';

/**
 * Backend lint issues for the graph the canvas holds now. Lints when the
 * flow opens and after each pause in editing. A failed call shows no
 * backend issues; lints never block Run or Save.
 */
export function useBackendFlowLints(
  collection: string | null,
  flow: Flow | null,
  delayMs: number = LINT_DEBOUNCE_MS,
): FlowIssue[] {
  const [issues, setIssues] = useState<FlowIssue[]>([]);
  // Bumped by every cleanup, so a response from an older effect is ignored.
  const generationRef = useRef(0);
  // Read when the timer fires, so the effect depends on the graph's content only.
  const flowRef = useRef(flow);
  flowRef.current = flow;
  const key = collection && flow ? JSON.stringify(flow) : null;

  useEffect(() => {
    const generation = generationRef.current;
    if (!collection || key === null) {
      setIssues([]);
      return;
    }
    const timer = setTimeout(() => {
      const current = flowRef.current;
      if (!current) return;
      lintFlow(collection, current)
        .then((lints) => {
          if (generationRef.current === generation) setIssues(lints.map(toFlowIssue));
        })
        .catch((err: unknown) => {
          if (generationRef.current !== generation) return;
          console.warn('[flow-lint] lint_flow failed', err);
          setIssues([]);
        });
    }, delayMs);
    return () => {
      clearTimeout(timer);
      generationRef.current += 1;
    };
  }, [collection, key, delayMs]);

  return issues;
}
