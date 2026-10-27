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
  ['This is already the spec link.', 'This is already the spec link.'],
  [
    'This collection is not connected to an OpenAPI spec.',
    'This collection is not connected to an OpenAPI spec.',
  ],
  [
    'Changing the link failed. Nothing was changed.',
    'Changing the link failed. Nothing was changed.',
  ],
  [
    'An earlier sync is not finished. Try again after Rocket restarts.',
    'An earlier sync is not finished. Try again after Rocket restarts.',
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
  [
    'These requests cannot be reset because the sync data is from an older version.',
    'These requests cannot be reset because the sync data is from an older version.',
  ],
  [
    'A sync is running. Try again when it is done.',
    'A sync is running. Try again when it is done.',
  ],
  [
    'The request changed on disk. Reload and try again.',
    'The request changed on disk. Reload and try again.',
  ],
  ['There is no backup for this request.', 'There is no backup for this request.'],
  ['Reset failed. Nothing was changed.', 'Reset failed. Nothing was changed.'],
  [
    'The file was not found, or it needs a sign-in.',
    'The file was not found, or it needs a sign-in. For a private GitHub repository, add an access token.',
  ],
  [
    'or the token was rejected or lacks Contents read access.',
    'The file was not found, or the token was rejected or lacks Contents read access.',
  ],
  ['The file is larger than the 25 MB limit.', 'The file is larger than the 25 MB limit.'],
  [
    'A newer spec is waiting but its copy is missing.',
    'A newer spec is waiting but its copy is missing. Run Check now.',
  ],
  [
    'The collection changed. Run the preview again.',
    'The collection changed. Run the preview again.',
  ],
  ['The spec changed. Run the preview again.', 'The spec changed. Run the preview again.'],
  [
    'This collection is already connected to an OpenAPI spec.',
    'This collection is already connected to an OpenAPI spec.',
  ],
  ['Enter the spec link.', 'Enter the spec link.'],
  ['The file is not a supported OpenAPI spec.', 'The file is not a supported OpenAPI spec.'],
  ['The spec could not be loaded.', 'The spec could not be loaded.'],
  ['The file could not be fetched.', 'The file could not be fetched.'],
  ['A request or an operation is chosen twice.', 'A request or an operation is chosen twice.'],
  [
    'A row of the review is not complete. Run the preview again.',
    'A row of the review is not complete. Run the preview again.',
  ],
  ['The collection was not found.', 'The collection was not found.'],
  ['Connecting failed. Nothing was changed.', 'Connecting failed. Nothing was changed.'],
  [
    'openapi-reset-backups.json',
    'The reset backups file is damaged. Remove .rocket/openapi-reset-backups.json from the collection folder, then try again.',
  ],
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

/** True when the collection or the spec changed after the preview, so the preview is out of date. */
export function isStaleConnect(error: unknown): boolean {
  const text = rawText(error);
  return text?.includes('Run the preview again.') ?? false;
}

const CONNECT_WARNINGS = ['Checks are not enabled yet.', 'The token was not saved.'];

/** A warning of a finished connect. Only the two fixed sentences pass; anything else is dropped. */
export function connectWarningMessage(warning: string): string | null {
  return CONNECT_WARNINGS.includes(warning) ? warning : null;
}

const CHANGE_LINK_WARNINGS = [
  'The token was not saved.',
  'The old token could not be removed.',
  'The new link needs your approval before checks run.',
];

/** A warning of a finished link change. Only the fixed sentences pass; anything else is dropped. */
export function changeLinkWarningMessage(warning: string): string | null {
  return CHANGE_LINK_WARNINGS.includes(warning) ? warning : null;
}
