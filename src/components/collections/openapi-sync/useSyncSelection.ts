import { type Dispatch, type SetStateAction, useCallback, useMemo, useState } from 'react';
import type { OpenApiSelection, OpenApiSyncPlan } from '@/lib/tauri-api';
import { buildSelection } from './buildSelection';
import { acceptAllSelection, countOverwrites, defaultSelection } from './bulkSelection';

type Links = Array<[string, string]>;
type Choice = 'spec' | 'yours';
type Choices = Map<string, Record<string, Choice>>;

interface State {
  approved: Set<string>;
  skips: Set<string>;
  variables: Set<string>;
  choices: Choices;
}

const emptyState = (): State => ({
  approved: new Set(),
  skips: new Set(),
  variables: new Set(),
  choices: new Map(),
});

/** The state a plan starts with, see `defaultSelection`. */
const defaultState = (plan: OpenApiSyncPlan | undefined): State => ({
  ...emptyState(),
  ...(plan ? defaultSelection(plan) : {}),
});

const EMPTY_PLAN: OpenApiSyncPlan = {
  pending: false,
  items: [],
  variableChanges: [],
  unchanged: [],
  skippedHidden: 0,
  deletedByUser: [],
  cautious: false,
  warnings: [],
};

/** A string that changes whenever the set of item or variable digests changes. */
function planSignature(plan: OpenApiSyncPlan): string {
  const items = plan.items.map((i) => `${i.kind}\u0000${i.key}\u0000${i.digest}`);
  const vars = plan.variableChanges.map((v) => `v\u0000${v.name}\u0000${v.digest}`);
  return [...items, ...vars].join('\u0001');
}

/** True when the plan has the rename hint for the pair, or the merged Changed item under the new key. */
function linkIsShown(plan: OpenApiSyncPlan, oldKey: string, newKey: string): boolean {
  // A rename with no content change merges to nothing and is listed as unchanged.
  if (plan.unchanged.some((u) => u.key === oldKey)) return true;
  return plan.items.some(
    (i) =>
      (i.kind === 'changed' && i.key === newKey) ||
      (i.kind === 'new' && i.key === newKey && i.possiblyRenamedFrom === oldKey) ||
      (i.kind === 'removed' && i.key === oldKey && i.possiblyRenamedTo === newKey),
  );
}

function withKeys(set: Set<string>, keys: Iterable<string>, on: boolean): Set<string> {
  const next = new Set(set);
  for (const k of keys) {
    if (on) next.add(k);
    else next.delete(k);
  }
  return next;
}

/**
 * The selection of one OpenAPI sync tab. It lives in the tab, not in a store. The rename links
 * are owned by the caller because they are part of the plan query key. A plan with a new
 * digest set resets the selection to the defaults, so an approval never outlives the item it
 * was made for. Inside one plan the user's choices are kept.
 */
export function useSyncSelection(
  plan: OpenApiSyncPlan | undefined,
  links: Links,
  setLinks: Dispatch<SetStateAction<Links>>,
) {
  const [state, setState] = useState<State>(() => defaultState(plan));
  const signature = useMemo(() => (plan ? planSignature(plan) : ''), [plan]);
  const [seen, setSeen] = useState(signature);
  if (seen !== signature) {
    setSeen(signature);
    setState(defaultState(plan));
    // Links stay across new plans. Only the ones the plan no longer shows are dropped.
    if (plan && links.length > 0) {
      const valid = links.filter(([oldKey, newKey]) => linkIsShown(plan, oldKey, newKey));
      if (valid.length !== links.length) setLinks(valid);
    }
  }

  const toggle = useCallback((key: string) => {
    setState((s) => {
      const on = !s.approved.has(key);
      return {
        ...s,
        approved: withKeys(s.approved, [key], on),
        skips: on ? withKeys(s.skips, [key], false) : s.skips,
      };
    });
  }, []);

  const setMany = useCallback((keys: string[], on: boolean) => {
    setState((s) => ({
      ...s,
      approved: withKeys(s.approved, keys, on),
      skips: on ? withKeys(s.skips, keys, false) : s.skips,
    }));
  }, []);

  const skip = useCallback((keys: string[]) => {
    setState((s) => ({
      ...s,
      approved: withKeys(s.approved, keys, false),
      skips: withKeys(s.skips, keys, true),
    }));
  }, []);

  const unskip = useCallback((keys: string[]) => {
    setState((s) => ({ ...s, skips: withKeys(s.skips, keys, false) }));
  }, []);

  const toggleVariable = useCallback((name: string) => {
    setState((s) => ({ ...s, variables: withKeys(s.variables, [name], !s.variables.has(name)) }));
  }, []);

  const setManyVariables = useCallback((names: string[], on: boolean) => {
    setState((s) => ({ ...s, variables: withKeys(s.variables, names, on) }));
  }, []);

  const setChoice = useCallback((key: string, field: string, choice: Choice) => {
    setState((s) => {
      const choices = new Map(s.choices);
      choices.set(key, { ...choices.get(key), [field]: choice });
      return { ...s, choices };
    });
  }, []);

  const linkRename = useCallback(
    (oldKey: string, newKey: string) => {
      setLinks((prev) => (prev.some(([o]) => o === oldKey) ? prev : [...prev, [oldKey, newKey]]));
    },
    [setLinks],
  );

  const unlink = useCallback(
    (oldKey: string) => setLinks((prev) => prev.filter(([o]) => o !== oldKey)),
    [setLinks],
  );

  const clearLinks = useCallback(() => setLinks([]), [setLinks]);

  const acceptAll = useCallback((): { overwritten: number } => {
    const overwritten = plan ? countOverwrites(plan) : 0;
    if (plan) {
      setState((s) => {
        const next = acceptAllSelection(plan, s.approved, s.choices);
        // The accepted items must not stay skipped.
        return { ...s, ...next, skips: withKeys(s.skips, next.approved, false) };
      });
    }
    return { overwritten };
  }, [plan]);

  const keepCurrent = useCallback(() => {
    setState((s) => ({ ...s, approved: new Set(), variables: new Set() }));
  }, []);

  const skipAll = useCallback(() => {
    setState((s) => ({
      ...s,
      approved: new Set(),
      variables: new Set(),
      skips: new Set(plan ? plan.items.map((i) => i.key) : []),
    }));
  }, [plan]);

  const reset = useCallback(() => setState(emptyState()), []);

  const selection: OpenApiSelection = useMemo(
    () => buildSelection(plan ?? EMPTY_PLAN, { ...state, links }),
    [plan, state, links],
  );

  return {
    approved: state.approved,
    toggle,
    setMany,
    choices: state.choices,
    setChoice,
    skips: state.skips,
    skip,
    unskip,
    variables: state.variables,
    toggleVariable,
    setManyVariables,
    links,
    linkRename,
    unlink,
    clearLinks,
    reset,
    selection,
    selectedCount: state.approved.size + state.variables.size,
    acceptAll,
    keepCurrent,
    skipAll,
  };
}
