import type { AuthState } from '@/types/pane-types';

export interface AuthTypeOption {
  label: string;
  value: AuthState['authType'];
}

// Auth types that can be kept and sent but not picked from the list. They show up in the
// selector only while the request already uses one, so its current value always has a label.
const READ_ONLY_OPTIONS: AuthTypeOption[] = [
  { label: 'NTLM', value: 'ntlm' },
  { label: 'OAuth 1.0', value: 'oauth1' },
];

export function withCurrentAuthType(
  options: AuthTypeOption[],
  current: AuthState['authType'],
): AuthTypeOption[] {
  const extra = READ_ONLY_OPTIONS.find((o) => o.value === current);
  return extra && !options.some((o) => o.value === current) ? [...options, extra] : options;
}
