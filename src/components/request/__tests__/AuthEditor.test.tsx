import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { AuthState } from '@/types/pane-types';

// CodeMirror does not run in jsdom, so the variable-aware field is replaced by a plain input.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    'aria-label': label,
  }: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => <input aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} />,
}));
vi.mock('../oauth2/OAuth2AuthEditor', () => ({ OAuth2AuthEditor: () => null }));

import { AuthEditor } from '../AuthEditor';

describe('AuthEditor for digest, wsse, ntlm and oauth1', () => {
  it.each(['digest', 'wsse'] as const)('edits the %s username and password', (authType) => {
    const auth: AuthState = { authType, [authType]: { username: 'u', password: 'p' } };
    const onChange = vi.fn();
    render(<AuthEditor auth={auth} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText('Username'), { target: { value: 'bob' } });
    expect(onChange).toHaveBeenLastCalledWith({
      authType,
      [authType]: { username: 'bob', password: 'p' },
    });

    fireEvent.change(screen.getByLabelText('Password'), { target: { value: 'pw' } });
    expect(onChange).toHaveBeenLastCalledWith({
      authType,
      [authType]: { username: 'u', password: 'pw' },
    });
  });

  it('shows a read-only note for ntlm and does not offer fields', () => {
    const auth: AuthState = {
      authType: 'ntlm',
      ntlm: { username: 'u', password: 'p', domain: 'CORP' },
    };
    render(<AuthEditor auth={auth} onChange={vi.fn()} />);
    expect(screen.getByText(/NTLM authentication is not supported yet/)).toBeTruthy();
    expect(screen.queryByLabelText('Username')).toBeNull();
  });

  it('shows the oauth1 editor', () => {
    render(
      <AuthEditor
        auth={{ authType: 'oauth1', oauth1: { consumerKey: 'ck' } }}
        onChange={vi.fn()}
      />,
    );
    expect(screen.getByLabelText('Consumer key')).toHaveValue('ck');
  });
});

describe('AuthEditor for aws-sig-v4', () => {
  it('edits the profile name and keeps the other fields', () => {
    const onChange = vi.fn();
    render(
      <AuthEditor
        auth={{
          authType: 'aws-sig-v4',
          awsSigV4: { accessKey: 'a', secretKey: 's', region: 'r', service: 'x', sessionToken: '' },
        }}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByLabelText('Profile name'), { target: { value: 'prod' } });
    expect(onChange).toHaveBeenLastCalledWith({
      authType: 'aws-sig-v4',
      awsSigV4: {
        accessKey: 'a',
        secretKey: 's',
        region: 'r',
        service: 'x',
        sessionToken: '',
        profileName: 'prod',
      },
    });
  });
});
