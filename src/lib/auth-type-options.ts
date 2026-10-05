import type { AuthState } from '@/types/pane-types';

export interface AuthTypeOption {
  label: string;
  value: AuthState['authType'];
}

// OAuth 1.0 has an editor, so callers list it like any other type.
export const OAUTH1_OPTION: AuthTypeOption = { label: 'OAuth 1.0', value: 'oauth1' };

// Auth types that can be kept and sent but not picked from the list. They show up in the
// selector only while the request already uses one, so its current value always has a label.
const READ_ONLY_OPTIONS: AuthTypeOption[] = [{ label: 'NTLM', value: 'ntlm' }];

export function withCurrentAuthType(
  options: AuthTypeOption[],
  current: AuthState['authType'],
): AuthTypeOption[] {
  const extra = READ_ONLY_OPTIONS.find((o) => o.value === current);
  return extra && !options.some((o) => o.value === current) ? [...options, extra] : options;
}
