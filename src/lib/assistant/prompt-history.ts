/** Prompts kept per workspace for Up and Down recall. */
export const PROMPT_HISTORY_LIMIT = 50;

/** Longest prompt kept in the history, in characters. */
export const PROMPT_HISTORY_ENTRY_MAX = 4000;

const keyFor = (workspaceId: string) => `rocket-api:assistant-prompt-history:${workspaceId}`;

/** The workspace's saved prompts, oldest first. Empty when storage is missing or broken. */
export function loadPromptHistory(workspaceId: string): string[] {
  if (!workspaceId) return [];
  try {
    const raw = localStorage.getItem(keyFor(workspaceId));
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((entry): entry is string => typeof entry === 'string')
      .map((entry) => entry.slice(0, PROMPT_HISTORY_ENTRY_MAX))
      .slice(-PROMPT_HISTORY_LIMIT);
  } catch {
    // Storage can be blocked or hold bad JSON. History is a convenience, so start empty.
    return [];
  }
}

/** `history` with `prompt` added as the newest entry. A repeat moves to the end. */
export function appendPromptHistory(history: readonly string[], prompt: string): string[] {
  const text = prompt.trim().slice(0, PROMPT_HISTORY_ENTRY_MAX);
  if (text === '') return [...history];
  return [...history.filter((entry) => entry !== text), text].slice(-PROMPT_HISTORY_LIMIT);
}

/** Saves the history. A storage failure only loses the history. */
export function savePromptHistory(workspaceId: string, history: readonly string[]): void {
  if (!workspaceId) return;
  try {
    localStorage.setItem(keyFor(workspaceId), JSON.stringify(history.slice(-PROMPT_HISTORY_LIMIT)));
  } catch {
    // Storage can be full or blocked. The prompt was still sent.
  }
}

/** Forgets the workspace's saved prompts. */
export function clearPromptHistory(workspaceId: string): void {
  if (!workspaceId) return;
  try {
    localStorage.removeItem(keyFor(workspaceId));
  } catch {
    // Storage can be blocked. There is nothing else to clear.
  }
}
