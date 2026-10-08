import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FlowEdge, InlineRequestData } from '@/lib/tauri-api';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { FlowVariableContextProvider } from '../flowVariableContext';
import { InlineSourceEditor } from '../InlineSourceEditor';

const seen = vi.hoisted(() => ({
  single: [] as {
    value: string;
    'aria-label'?: string;
    variableContext?: Map<string, unknown>;
    readOnlyVariables?: boolean;
    hoverPreview?: boolean;
  }[],
  monaco: [] as { variableContext?: Map<string, unknown> }[],
}));

vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
    variableContext?: Map<string, unknown>;
    readOnlyVariables?: boolean;
    hoverPreview?: boolean;
  }) => {
    seen.single.push(props);
    return (
      <input
        aria-label={props['aria-label']}
        value={props.value}
        onChange={(e) => props.onChange(e.target.value)}
      />
    );
  },
}));

// Monaco cannot run in jsdom. A textarea with the same value/onChange
// contract stands in for the body editor.
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: (props: {
    value: string;
    onChange?: (v: string) => void;
    variableContext?: Map<string, unknown>;
  }) => {
    seen.monaco.push(props);
    return (
      <textarea
        aria-label='Body'
        value={props.value}
        onChange={(e) => props.onChange?.(e.target.value)}
      />
    );
  },
}));

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

const request: InlineRequestData = {
  method: 'POST',
  url: '{{baseUrl}}/login',
  headers: [
    { name: 'Content-Type', value: 'application/json' },
    { name: 'X-Trace', value: 'on' },
  ],
  body: '{}',
};

function renderEditor(r = request, outOfRangeWires: FlowEdge[] = []) {
  const onChange = vi.fn();
  render(<InlineSourceEditor request={r} onChange={onChange} outOfRangeWires={outOfRangeWires} />);
  return onChange;
}

describe('InlineSourceEditor', () => {
  it('edits the URL', async () => {
    const onChange = renderEditor();
    await userEvent.type(screen.getByLabelText('URL'), 'x');
    expect(onChange).toHaveBeenLastCalledWith({ ...request, url: '{{baseUrl}}/loginx' });
  });

  it('changes the method', async () => {
    const onChange = renderEditor();
    await userEvent.click(screen.getByRole('combobox', { name: 'Method' }));
    await userEvent.click(await screen.findByRole('option', { name: 'PUT' }));
    expect(onChange).toHaveBeenLastCalledWith({ ...request, method: 'PUT' });
  });

  it('keeps an unlisted method visible and selectable', async () => {
    renderEditor({ ...request, method: 'PROPFIND' });
    const trigger = screen.getByRole('combobox', { name: 'Method' });
    expect(trigger).toHaveTextContent('PROPFIND');
    await userEvent.click(trigger);
    expect(await screen.findByRole('option', { name: 'PROPFIND' })).toBeInTheDocument();
  });

  it('adds, renames and removes headers', async () => {
    const onChange = renderEditor();
    await userEvent.click(screen.getByRole('button', { name: 'Add header' }));
    expect(onChange).toHaveBeenLastCalledWith({
      ...request,
      headers: [...request.headers, { name: '', value: '' }],
    });

    await userEvent.type(screen.getByLabelText('Header 2 value'), '!');
    expect(onChange).toHaveBeenLastCalledWith({
      ...request,
      headers: [request.headers[0], { name: 'X-Trace', value: 'on!' }],
    });

    await userEvent.click(screen.getByRole('button', { name: 'Remove header 1' }));
    expect(onChange).toHaveBeenLastCalledWith({ ...request, headers: [request.headers[1]] });
  });

  it('stores an emptied body as null', async () => {
    const onChange = renderEditor({ ...request, body: 'x' });
    await userEvent.clear(screen.getByLabelText('Body'));
    expect(onChange).toHaveBeenLastCalledWith({ ...request, body: null });
  });

  it('warns about a wire whose header index no longer exists', () => {
    const wire: FlowEdge = {
      id: 'e1',
      sourceNodeId: 'a',
      targetNodeId: 'r1',
      targetField: 'headers[2].value',
      expression: 'response.body',
    };
    renderEditor(request, [wire]);
    expect(screen.getByRole('alert')).toHaveTextContent(
      'A wire targets header position 3, which no longer exists. This node will fail when the flow runs.',
    );
  });

  it('shows no warning when every index wire still has its header', () => {
    renderEditor();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});

describe('InlineSourceEditor variable context', () => {
  const ctx = new Map<string, VariableScopeEntry>([
    ['baseUrl', { value: 'https://a.test', source: 'environment', label: 'dev', secret: false }],
  ]);
  const latest = (label: string) => [...seen.single].reverse().find((p) => p['aria-label'] === label);

  beforeEach(() => {
    seen.single.length = 0;
    seen.monaco.length = 0;
  });

  it('gives the URL and header editors the flow context, read-only for saving', () => {
    render(
      <FlowVariableContextProvider value={ctx}>
        <InlineSourceEditor request={request} onChange={vi.fn()} outOfRangeWires={[]} />
      </FlowVariableContextProvider>,
    );
    for (const label of ['URL', 'Header 1 name', 'Header 1 value']) {
      expect(latest(label)?.variableContext).toBe(ctx);
      expect(latest(label)?.readOnlyVariables).toBe(true);
    }
  });

  it('gives the body editor the flow context', () => {
    render(
      <FlowVariableContextProvider value={ctx}>
        <InlineSourceEditor request={request} onChange={vi.fn()} outOfRangeWires={[]} />
      </FlowVariableContextProvider>,
    );
    expect(seen.monaco[seen.monaco.length - 1]?.variableContext).toBe(ctx);
  });

  it('passes no context outside a provider', () => {
    render(<InlineSourceEditor request={request} onChange={vi.fn()} outOfRangeWires={[]} />);
    expect(latest('URL')?.variableContext).toBeUndefined();
  });
});
