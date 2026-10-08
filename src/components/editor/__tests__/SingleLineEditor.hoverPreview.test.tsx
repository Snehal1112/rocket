import { render } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';

const hover = vi.hoisted(() => ({ variableHover: vi.fn(() => []) }));
vi.mock('../extensions/variable-hover', () => hover);
vi.mock('@/hooks/useVariableCommit', () => ({ useVariableCommit: () => vi.fn() }));

import { SingleLineEditor } from '../SingleLineEditor';

const context = new Map<string, VariableScopeEntry>([
  ['host', { value: 'a.test', source: 'environment', label: 'dev', secret: false }],
]);

describe('SingleLineEditor hoverPreview', () => {
  beforeEach(() => hover.variableHover.mockClear());

  it('does not install the hover by default, so other editors are unchanged', () => {
    render(<SingleLineEditor value='{{host}}' onChange={vi.fn()} variableContext={context} />);
    expect(hover.variableHover).not.toHaveBeenCalled();
  });

  it('installs the hover when hoverPreview is set with a variable context', () => {
    render(
      <SingleLineEditor value='{{host}}' onChange={vi.fn()} variableContext={context} hoverPreview />,
    );
    expect(hover.variableHover).toHaveBeenCalled();
  });

  it('does not install the hover without a variable context', () => {
    render(<SingleLineEditor value='{{host}}' onChange={vi.fn()} hoverPreview />);
    expect(hover.variableHover).not.toHaveBeenCalled();
  });
});
