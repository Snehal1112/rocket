import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { SingleLineEditor } from '../SingleLineEditor';

vi.mock('@/stores/env-store', () => ({
  useEnvStore: (selector: (s: unknown) => unknown) =>
    selector({
      activeEnvId: null,
      environments: [],
      globalEnv: null,
      updateEnvironment: async () => {
        /* stub */
      },
      updateGlobalEnvironment: async () => {
        /* stub */
      },
    }),
}));

function makeContext(
  entries: Record<string, Pick<VariableScopeEntry, 'source' | 'value'>>,
): Map<string, VariableScopeEntry> {
  const map = new Map<string, VariableScopeEntry>();
  for (const [key, val] of Object.entries(entries)) {
    map.set(key, { value: val.value, source: val.source, label: val.source, secret: false });
  }
  return map;
}

describe('SingleLineEditor', () => {
  it('renders without crashing', () => {
    const { container } = render(<SingleLineEditor value='hello' onChange={vi.fn()} />);
    expect(container.querySelector('.cm-editor')).not.toBeNull();
  });

  it('displays the initial value', () => {
    const { container } = render(<SingleLineEditor value='test content' onChange={vi.fn()} />);
    const content = container.querySelector('.cm-content');
    expect(content?.textContent).toContain('test content');
  });

  it('renders placeholder when value is empty', () => {
    const { container } = render(
      <SingleLineEditor value='' onChange={vi.fn()} placeholder='Enter URL' />,
    );
    const ph = container.querySelector('.cm-placeholder');
    expect(ph?.textContent).toBe('Enter URL');
  });

  it('applies variable highlighting when context is provided', () => {
    const ctx = makeContext({
      baseUrl: { source: 'environment', value: 'https://api.example.com' },
    });
    const { container } = render(
      <SingleLineEditor value='https://{{baseUrl}}/api' onChange={vi.fn()} variableContext={ctx} />,
    );
    const envVars = container.querySelectorAll('.cm-var-environment');
    expect(envVars.length).toBe(1);
  });

  it('applies unresolved class for unknown variables', () => {
    const { container } = render(
      <SingleLineEditor value='{{unknown}}' onChange={vi.fn()} variableContext={new Map()} />,
    );
    const unresolved = container.querySelectorAll('.cm-var-unresolved');
    expect(unresolved.length).toBe(1);
  });

  it('does not render variable extensions when context is undefined', () => {
    const { container } = render(<SingleLineEditor value='{{someVar}}' onChange={vi.fn()} />);
    // No variable decorations applied.
    expect(container.querySelectorAll('.cm-var').length).toBe(0);
  });
});
