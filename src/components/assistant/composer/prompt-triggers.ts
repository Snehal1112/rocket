/** A `#` reference or `/` command the user is typing at the cursor. */
export interface TriggerMatch {
  kind: 'reference' | 'command';
  /** Document offset of the `#` or `/` character. */
  from: number;
  /** Text typed after the trigger character. */
  query: string;
}

// A `#` at the start of the line or after whitespace, then text without spaces.
const REFERENCE = /(?:^|\s)#([^\s#]*)$/;
// A `/` at the very start of the prompt, then a command name.
const COMMAND = /^\/([\w-]*)$/;

/**
 * Finds the trigger being typed. `textBefore` is the line text up to the cursor and
 * `lineFrom` is the document offset where that line starts. A `/` command counts
 * only on the first line, at the start of the prompt.
 */
export function matchTrigger(textBefore: string, lineFrom: number): TriggerMatch | null {
  const reference = REFERENCE.exec(textBefore);
  if (reference) {
    const query = reference[1];
    return {
      kind: 'reference',
      from: lineFrom + textBefore.length - query.length - 1,
      query,
    };
  }
  if (lineFrom === 0) {
    const command = COMMAND.exec(textBefore);
    if (command) return { kind: 'command', from: 0, query: command[1] };
  }
  return null;
}
