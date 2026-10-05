import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

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

import { OAuth1AuthEditor } from '../OAuth1AuthEditor';

describe('OAuth1AuthEditor', () => {
  it('edits a field and keeps the fields it does not know', () => {
    const onChange = vi.fn();
    render(
      <OAuth1AuthEditor
        value={{ consumerKey: 'ck', signatureMethod: 'HMAC-SHA1', privateKey: { type: 'text' } }}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByLabelText('Consumer secret'), { target: { value: 's3' } });
    expect(onChange).toHaveBeenLastCalledWith({
      consumerKey: 'ck',
      signatureMethod: 'HMAC-SHA1',
      privateKey: { type: 'text' },
      consumerSecret: 's3',
    });
  });

  it('removes a field when it is emptied instead of saving an empty string', () => {
    const onChange = vi.fn();
    render(<OAuth1AuthEditor value={{ consumerKey: 'ck', realm: 'r' }} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText('Realm'), { target: { value: '' } });
    expect(onChange).toHaveBeenLastCalledWith({ consumerKey: 'ck' });
  });

  it('toggles the body hash flag', () => {
    const onChange = vi.fn();
    render(<OAuth1AuthEditor value={{}} onChange={onChange} />);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Include body hash' }));
    expect(onChange).toHaveBeenLastCalledWith({ includeBodyHash: true });
  });
});
