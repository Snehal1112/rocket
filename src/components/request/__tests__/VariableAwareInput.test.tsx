import { render, screen } from '@testing-library/react';
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
      updateEnvironment: async () => {
        /* no-op */
      },
      updateGlobalEnvironment: async () => {
        /* no-op */
      },
    }),
}));

function makeContext(entries: Record<string, VariableScopeEntry>): Map<string, VariableScopeEntry> {
  return new Map(Object.entries(entries));
}

describe('VariableAwareInput', () => {
  it('renders a plain input when variableContext is undefined', () => {
    render(<VariableAwareInput value='hello' onChange={vi.fn()} />);
    const input = screen.getByRole('textbox');
    expect(input).toBeDefined();
    // No overlay present when no context.
    expect(document.querySelector('[aria-hidden="true"]')).toBeNull();
  });

  it('renders overlay when variableContext is provided', () => {
    render(
      <VariableAwareInput
        value='Bearer {{token}}'
        onChange={vi.fn()}
        variableContext={makeContext({
          token: { value: 'abc123', source: 'environment', label: 'Dev', secret: false },
        })}
      />,
    );
    expect(document.querySelector('[aria-hidden="true"]')).not.toBeNull();
  });

  it('renders resolved variable with source color span', () => {
    render(
      <VariableAwareInput
        value='{{token}}'
        onChange={vi.fn()}
        variableContext={makeContext({
          token: { value: 'abc', source: 'environment', label: 'Dev', secret: false },
        })}
      />,
    );
    const overlay = document.querySelector('[aria-hidden="true"]');
    expect(overlay?.textContent).toContain('{{token}}');
    // The variable span should have a highlight class.
    const span = overlay?.querySelector('.rounded-sm');
    expect(span).not.toBeNull();
  });

  it('renders unresolved variable with destructive color span', () => {
    render(
      <VariableAwareInput
        value='{{missing}}'
        onChange={vi.fn()}
        variableContext={makeContext({})}
      />,
    );
    const overlay = document.querySelector('[aria-hidden="true"]');
    const span = overlay?.querySelector('.text-destructive');
    expect(span).not.toBeNull();
  });

  it('renders plain text in overlay when value has no variables', () => {
    render(
      <VariableAwareInput
        value='plain text'
        onChange={vi.fn()}
        variableContext={makeContext({})}
      />,
    );
    const overlay = document.querySelector('[aria-hidden="true"]');
    // Overlay renders but contains only a text span.
    expect(overlay?.textContent).toBe('plain text');
  });
});
