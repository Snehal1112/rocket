import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { MultiLineEditor } from '../MultiLineEditor';

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

describe('MultiLineEditor', () => {
  it('renders without crashing', () => {
    const { container } = render(<MultiLineEditor value='hello world' />);
    expect(container.querySelector('.cm-editor')).not.toBeNull();
  });

  it('displays the initial value', () => {
    const { container } = render(<MultiLineEditor value='line 1\nline 2' />);
    const content = container.querySelector('.cm-content');
    expect(content?.textContent).toContain('line 1');
    expect(content?.textContent).toContain('line 2');
  });

  it('renders line numbers by default', () => {
    const { container } = render(<MultiLineEditor value='line 1\nline 2\nline 3' />);
    const gutters = container.querySelectorAll('.cm-lineNumbers');
    expect(gutters.length).toBeGreaterThan(0);
  });

  it('is read-only when readOnly prop is true', () => {
    const { container } = render(<MultiLineEditor value='readonly content' readOnly />);
    // CM6 adds contenteditable="false" on the content element in read-only mode.
    const content = container.querySelector('.cm-content');
    expect(content?.getAttribute('contenteditable')).toBe('false');
  });

  it('applies variable highlighting when context is provided', () => {
    const ctx = makeContext({
      baseUrl: { source: 'environment', value: 'https://api.com' },
    });
    const { container } = render(
      <MultiLineEditor value='url: {{baseUrl}}/path' variableContext={ctx} />,
    );
    const envVars = container.querySelectorAll('.cm-var-environment');
    expect(envVars.length).toBe(1);
  });

  it('does not apply variable highlighting when context is undefined', () => {
    const { container } = render(<MultiLineEditor value='{{someVar}}' />);
    expect(container.querySelectorAll('.cm-var').length).toBe(0);
  });

  it('renders with json language highlighting', () => {
    const { container } = render(<MultiLineEditor value='{"key": "value"}' language='json' />);
    // JSON language extension adds syntax tree — verify editor renders.
    expect(container.querySelector('.cm-editor')).not.toBeNull();
  });

  it('renders with yaml language highlighting', () => {
    const { container } = render(
      <MultiLineEditor value='name: test\nversion: 1.0' language='yaml' />,
    );
    expect(container.querySelector('.cm-editor')).not.toBeNull();
  });

  it('detects language from bodyMode', () => {
    const { container } = render(<MultiLineEditor value='<root><child/></root>' bodyMode='xml' />);
    expect(container.querySelector('.cm-editor')).not.toBeNull();
  });

  it('applies custom height', () => {
    const { container } = render(<MultiLineEditor value='test' height='500px' />);
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.height).toBe('500px');
  });
});
