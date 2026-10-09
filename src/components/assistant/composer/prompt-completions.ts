import type {
  Completion,
  CompletionContext,
  CompletionResult,
  CompletionSource,
} from '@codemirror/autocomplete';
import type { EditorView } from '@codemirror/view';
import type { ReferenceItem, ReferenceKind, SlashCommandItem } from '@/lib/assistant/types';
import { matchTrigger, type TriggerMatch } from './prompt-triggers';

/** A value read at call time, such as a React ref, so the editor never holds a stale callback. */
export interface Latest<T> {
  readonly current: T;
}

const KIND_LABEL: Record<ReferenceKind, string> = {
  request: 'request',
  folder: 'folder',
  collection: 'collection',
  environment: 'environment',
  'last-response': 'response',
};

function triggerAt(context: CompletionContext): TriggerMatch | null {
  const line = context.state.doc.lineAt(context.pos);
  return matchTrigger(line.text.slice(0, context.pos - line.from), line.from);
}

/**
 * The `#` list. The source filters, so CodeMirror shows the items in the given order.
 * Picking an item removes the typed `#query` text and hands the item to `onPicked`,
 * which turns it into a chip.
 */
export function referenceCompletions(
  source: Latest<(query: string) => ReferenceItem[]>,
  onPicked: Latest<(item: ReferenceItem) => void>,
): CompletionSource {
  return (context: CompletionContext): CompletionResult | null => {
    const match = triggerAt(context);
    if (!match || match.kind !== 'reference') return null;
    const items = source.current(match.query);
    if (items.length === 0) return null;
    const options: Completion[] = items.map((item) => ({
      label: item.label,
      detail:
        item.kind === 'collection'
          ? KIND_LABEL[item.kind]
          : `${KIND_LABEL[item.kind]} · ${item.collection}`,
      type: 'reference',
      apply: (view: EditorView, _completion: Completion, from: number, to: number) => {
        view.dispatch({ changes: { from, to, insert: '' }, selection: { anchor: from } });
        onPicked.current(item);
      },
    }));
    return { from: match.from, options, filter: false };
  };
}

/** The `/` list. Picking a command replaces the typed `/query` with its template. */
export function commandCompletions(
  source: Latest<(query: string) => SlashCommandItem[]>,
): CompletionSource {
  return (context: CompletionContext): CompletionResult | null => {
    const match = triggerAt(context);
    if (!match || match.kind !== 'command') return null;
    const commands = source.current(match.query);
    if (commands.length === 0) return null;
    const options: Completion[] = commands.map((command) => ({
      label: `/${command.name}`,
      detail: command.description,
      type: 'command',
      apply: (view: EditorView, _completion: Completion, from: number, to: number) => {
        view.dispatch({
          changes: { from, to, insert: command.template },
          selection: { anchor: from + command.template.length },
        });
      },
    }));
    return { from: match.from, options, filter: false };
  };
}
