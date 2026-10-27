export const GENERIC_SYNC_ERROR = 'Something went wrong with the OpenAPI sync. Try again.';

// Each entry maps a known backend phrase to a fixed sentence. The raw text is never shown.
const KNOWN: Array<[string, string]> = [
  [
    'consent_required:',
    'Approve this spec link on this machine first. Rocket does not contact it until you do.',
  ],
  [
    'location_changed:',
    'The spec link in this collection changed. Approve the new link before Rocket contacts it.',
  ],
  [
    'The spec link changed. Check the link and approve it again.',
    'The spec link changed while you were looking at it. Check the link and approve it again.',
  ],
  [
    'cannot be approved because its link is ambiguous',
    'This spec link cannot be approved because it is ambiguous. Use a simpler link.',
  ],
  [
    'the pending spec changed, check again',
    'The new spec changed since this list was made. Check again to get the latest.',
  ],
  [
    'an earlier apply is not finished yet',
    'Another apply is not finished yet. Wait a moment and try again.',
  ],
  [
    'An apply is running on this collection. Try again when it finishes.',
    'An apply is running on this collection. Try again when it finishes.',
  ],
  [
    'The sync settings could not be removed. Try again.',
    'The sync settings could not be removed. Try again.',
  ],
  ['An apply is already running.', 'Another apply is still running. Wait for it to finish.'],
  ['Tokens work with GitHub links only.', 'Tokens work with GitHub links only.'],
  ['The token is empty.', 'Enter a token first.'],
  ['The saved spec cannot be shown.', 'The saved spec cannot be shown.'],
];

function rawText(error: unknown): string | null {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return null;
}

/** Turns a sync command error into a fixed sentence. Unknown text is never passed on. */
export function syncErrorMessage(error: unknown): string {
  const text = rawText(error);
  if (text === null) return GENERIC_SYNC_ERROR;
  for (const [needle, sentence] of KNOWN) {
    if (text.includes(needle)) return sentence;
  }
  return GENERIC_SYNC_ERROR;
}
