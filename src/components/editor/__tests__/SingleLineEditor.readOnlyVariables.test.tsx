import { EditorView } from '@codemirror/view';
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { openPopoverEffect } from '../extensions';
import { SingleLineEditor } from '../SingleLineEditor';

const commit = vi.hoisted(() => vi.fn(async () => undefined));
vi.mock('@/hooks/useVariableCommit', () => ({ useVariableCommit: () => commit }));

const entry: VariableScopeEntry = {
  value: 'https://a.test',
  source: 'environment',
  label: 'dev',
  secret: false,
};
const context = new Map([['baseUrl', entry]]);

// Opens the click popover the way a click on the token does.
function openPopover(container: HTMLElement) {
  const dom = container.querySelector<HTMLElement>('.cm-editor');
  const view = dom ? EditorView.findFromDOM(dom) : null;
  if (!view) throw new Error('The editor view was not found.');
  act(() => {
    view.dispatch({
      effects: openPopoverEffect.of({
        varName: 'baseUrl',
        from: 0,
        to: 11,
        tokenType: 'variable',
        entry,
      }),
    });
  });
}

async function popoverInput() {
  const dialog = await screen.findByRole('dialog', { hidden: true });
  return within(dialog).getByRole('textbox', { hidden: true });
}

describe('SingleLineEditor readOnlyVariables', () => {
  beforeEach(() => commit.mockClear());

  it('saves an edit made in the click popover by default', async () => {
    const { container } = render(
      <SingleLineEditor value='{{baseUrl}}' onChange={vi.fn()} variableContext={context} />,
    );
    openPopover(container);
    const input = await popoverInput();
    expect(input).not.toHaveAttribute('readonly');
    await userEvent.type(input, 'x{Enter}');
    expect(commit).toHaveBeenCalledWith('baseUrl', 'https://a.testx', 'environment');
  });

  it('shows the value but cannot save it when readOnlyVariables is set', async () => {
    const { container } = render(
      <SingleLineEditor
        value='{{baseUrl}}'
        onChange={vi.fn()}
        variableContext={context}
        readOnlyVariables
      />,
    );
    openPopover(container);
    const input = await popoverInput();
    expect(input).toHaveAttribute('readonly');
    expect(input).toHaveValue('https://a.test');
    await userEvent.type(input, 'x{Enter}');
    expect(commit).not.toHaveBeenCalled();
  });
});
