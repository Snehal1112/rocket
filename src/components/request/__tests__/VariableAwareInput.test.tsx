import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { VariableAwareInput } from '../VariableAwareInput';

vi.mock('@/stores/env-store', () => ({
  useEnvStore: (
    selector: (s: {
      activeEnvId: null;
      environments: never[];
      globalEnv: null;
      updateEnvironment: () => Promise<void>;
      updateGlobalEnvironment: () => Promise<void>;
    }) => unknown,
  ) =>
    selector({
      activeEnvId: null,
      environments: [],
      globalEnv: null,
      updateEnvironment: async () => {},
      updateGlobalEnvironment: async () => {},
    }),
}));

function makeContext(entries: Record<string, VariableScopeEntry>): Map<string, VariableScopeEntry> {
  return new Map(Object.entries(entries));
}

describe('VariableAwareInput', () => {
  it('renders a plain input when variableContext is undefined', () => {
    render(<VariableAwareInput value='hello' onChange={vi.fn()} />);
    // Plain <input> renders as textbox; no contenteditable present.
    expect(screen.getByRole('textbox')).toBeDefined();
    expect(document.querySelector('[contenteditable]')).toBeNull();
  });

  it('renders a plain input for type=password even with variableContext', () => {
    render(
      <VariableAwareInput
        value='secret'
        onChange={vi.fn()}
        type='password'
        variableContext={makeContext({})}
      />,
    );
    const input = document.querySelector('input[type="password"]');
    expect(input).not.toBeNull();
    expect(document.querySelector('[contenteditable]')).toBeNull();
  });

  it('renders a contenteditable editor when variableContext is provided', () => {
    render(
      <VariableAwareInput
        value='Bearer {{token}}'
        onChange={vi.fn()}
        variableContext={makeContext({
          token: { value: 'abc123', source: 'environment', label: 'Dev', secret: false },
        })}
      />,
    );
    expect(document.querySelector('[contenteditable]')).not.toBeNull();
    // No overlay div with aria-hidden.
    expect(document.querySelector('[aria-hidden="true"]')).toBeNull();
  });

  it('renders plain text content in the editor', () => {
    render(
      <VariableAwareInput
        value='plain text'
        onChange={vi.fn()}
        variableContext={makeContext({})}
      />,
    );
    const editor = document.querySelector('[contenteditable]')!;
    expect(editor.textContent).toBe('plain text');
  });

  it('renders a badge span for a resolved variable', () => {
    render(
      <VariableAwareInput
        value='{{token}}'
        onChange={vi.fn()}
        variableContext={makeContext({
          token: { value: 'abc', source: 'environment', label: 'Dev', secret: false },
        })}
      />,
    );
    const badge = document.querySelector('[data-badge]');
    expect(badge).not.toBeNull();
    expect(badge?.textContent).toBe('{{token}}');
  });

  it('renders a badge span with destructive class for an unresolved variable', () => {
    render(
      <VariableAwareInput
        value='{{missing}}'
        onChange={vi.fn()}
        variableContext={makeContext({})}
      />,
    );
    const badge = document.querySelector('[data-badge]');
    expect(badge?.className).toContain('text-destructive');
  });

  it('calls onChange when the editor content changes', () => {
    const onChange = vi.fn();
    render(
      <VariableAwareInput
        value='hello'
        onChange={onChange}
        variableContext={makeContext({})}
      />,
    );
    const editor = document.querySelector('[contenteditable]') as HTMLElement;
    editor.textContent = 'hello!';
    fireEvent(editor, new Event('input', { bubbles: true }));
    expect(onChange).toHaveBeenCalledWith('hello!');
  });
});
