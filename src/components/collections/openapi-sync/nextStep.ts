import type { OpenApiSyncStatus } from '@/lib/tauri-api';
import { isGithubLocation } from './openApiSyncView';

export interface NextStep {
  kind: 'add_token' | 'check_now' | 'change_link' | 'none';
  label: string;
}

const NONE: NextStep = { kind: 'none', label: '' };

/** What the user can do about the last failed check. An unknown code offers nothing. */
export function nextStep(status: OpenApiSyncStatus): NextStep {
  if (status.consent !== 'granted' || status.lastStatus?.state !== 'failed') return NONE;
  switch (status.lastStatus.code) {
    case 'not_found':
    case 'unauthorized':
      // Tokens work with GitHub links only.
      return isGithubLocation(status.location)
        ? { kind: 'add_token', label: 'Add access token' }
        : NONE;
    case 'network':
    case 'status':
    case 'rate_limited':
    case 'too_many_redirects':
      return { kind: 'check_now', label: 'Check now' };
    case 'file_missing':
      return { kind: 'change_link', label: 'Choose another file' };
    default:
      return NONE;
  }
}
