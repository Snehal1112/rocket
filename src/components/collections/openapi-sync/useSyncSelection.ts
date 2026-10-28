import { type Dispatch, type SetStateAction, useCallback, useMemo, useRef, useState } from 'react';
import type { OpenApiSelection, OpenApiSyncPlan } from '@/lib/tauri-api';
import { buildSelection } from './buildSelection';
import {
  acceptAllKeepingMine,
  acceptAllSelection,
  countOverwrites,
  defaultSelection,
} from './bulkSelection';

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

/**
 * The state for a new plan. An item or variable whose digest did not change keeps what the user
 * chose for it, so a plan that changes under them (a check, a skip, a rename link) never ticks
 * again something they unticked. Anything with new content starts from its default.
 */
function carryOver(prev: OpenApiSyncPlan, state: State, next: OpenApiSyncPlan): State {
  const fresh = defaultState(next);
  const was = new Map(prev.items.map((i) => [`${i.kind}\u0000${i.key}`, i.digest]));
  for (const item of next.items) {
    if (was.get(`${item.kind}\u0000${item.key}`) !== item.digest) continue;
    fresh.approved = withKeys(fresh.approved, [item.key], state.approved.has(item.key));
    fresh.skips = withKeys(fresh.skips, [item.key], state.skips.has(item.key));
    const chosen = state.choices.get(item.key);
    if (chosen) fresh.choices.set(item.key, chosen);
  }
  const wasVars = new Map(prev.variableChanges.map((v) => [v.name, v.digest]));
  for (const v of next.variableChanges) {
    if (wasVars.get(v.name) !== v.digest) continue;
    fresh.variables = withKeys(fresh.variables, [v.name], state.variables.has(v.name));
  }
  return fresh;
}

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
 * digest set resets an item whose digest changed to its defaults, so an approval never outlives
 * the content it was made for. Items whose digest did not change keep the user's choices, both
 * inside one plan and across plans.
 */
export function useSyncSelection(
  plan: OpenApiSyncPlan | undefined,
  links: Links,
  setLinks: Dispatch<SetStateAction<Links>>,
  collection?: string,
) {
  const [state, setState] = useState<State>(() => defaultState(plan));
  // Accept all keeps what the user changed, until they turn this off for this collection.
  const [keepMine, setKeepMine] = useState(true);
  const [seenCollection, setSeenCollection] = useState(collection);
  if (seenCollection !== collection) {
    setSeenCollection(collection);
    setKeepMine(true);
  }
  const signature = useMemo(() => (plan ? planSignature(plan) : ''), [plan]);
  const [seen, setSeen] = useState(signature);
  const [seenPlan, setSeenPlan] = useState(plan);
  const signatureRef = useRef(signature);
  signatureRef.current = signature;
  if (seen !== signature) {
    setSeen(signature);
    setSeenPlan(plan);
    setState((s) => (plan && seenPlan ? carryOver(seenPlan, s, plan) : defaultState(plan)));
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
    // With Keep my values on nothing the user changed is overwritten.
    const overwritten = plan && !keepMine ? countOverwrites(plan) : 0;
    if (plan) {
      setState((s) => {
        const next = keepMine
          ? acceptAllKeepingMine(plan, s.approved, s.choices)
          : acceptAllSelection(plan, s.approved, s.choices);
        // The accepted items must not stay skipped.
        return {
          ...s,
          ...next,
          // Variable changes are safe to take: a value the user changed is kept.
          variables: new Set(plan.variableChanges.map((v) => v.name)),
          skips: withKeys(s.skips, next.approved, false),
        };
      });
    }
    return { overwritten };
  }, [plan, keepMine]);

  const keepCurrent = useCallback(() => {
    // The conflict choices go back to keeping the user's version.
    setState((s) => ({
      ...s,
      approved: new Set(),
      variables: new Set(),
      choices: plan ? defaultSelection(plan).choices : new Map(),
    }));
  }, [plan]);

  const skipAll = useCallback((): (() => void) => {
    const before = state;
    const startedOn = signature;
    setState((s) => ({
      ...s,
      approved: new Set(),
      variables: new Set(),
      skips: new Set(plan ? plan.items.map((i) => i.key) : []),
    }));
    // The undo puts back the selection and the skips from before.
    // A new plan has its own defaults, so the old selection is not put back onto it.
    return () => {
      if (signatureRef.current !== startedOn) return;
      setState((s) => ({
        ...s,
        approved: before.approved,
        variables: before.variables,
        skips: before.skips,
      }));
    };
  }, [plan, state, signature]);

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
    keepMine,
    setKeepMine,
    keepCurrent,
    skipAll,
  };
}
