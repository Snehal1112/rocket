import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useEffect, useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { AgentConfigForm } from '@/components/settings/agent-config/AgentConfigForm';
import {
  type AgentFormState,
  emptyAgentForm,
  newRow,
} from '@/components/settings/agent-config/agent-config-form-state';
import { KIND_PRESETS } from '@/components/settings/agent-config/agent-config-validation';

const seenOnChange: unknown[] = [];

vi.mock('@/components/settings/agent-config/AgentCredentialSection', () => ({
  AgentCredentialSection: (props: {
    kind: string;
    value: { credentialSource: string; credentialEnvVar: string; sourceEnvVar: string };
    errors: { credentialEnvVar?: string | null };
    onChange: (patch: Record<string, string>) => void;
  }) => {
    seenOnChange.push(props.onChange);
    // Mirrors the real section: the source variable follows the key name until edited.
    const { value, onChange } = props;
    useEffect(() => {
      if (value.credentialSource !== 'env') return;
      if (value.sourceEnvVar !== value.credentialEnvVar) {
        onChange({ sourceEnvVar: value.credentialEnvVar });
      }
    }, [value.credentialSource, value.credentialEnvVar, value.sourceEnvVar, onChange]);
    return (
      <div data-testid='credential-section' data-kind={props.kind}>
        {props.errors.credentialEnvVar ?? ''}
        <button
          type='button'
          onClick={() => {
            props.onChange({ localCredential: 'tok' });
            props.onChange({ credentialEnvVar: 'MY_KEY' });
          }}
        >
          Patch twice
        </button>
      </div>
    );
  },
}));

function Harness({
  initial,
  onState,
}: {
  initial: AgentFormState;
  onState: (state: AgentFormState) => void;
}) {
  const [value, setValue] = useState(initial);
  const change = (next: AgentFormState) => {
    setValue(next);
    onState(next);
  };
  return <AgentConfigForm value={value} onChange={change} hasSavedCredential={false} />;
}

function setup(over: Partial<AgentFormState> = {}) {
  const states: AgentFormState[] = [];
  const initial = { ...emptyAgentForm('a1'), ...over };
  render(<Harness initial={initial} onState={(state) => states.push(state)} />);
  return { last: () => states[states.length - 1] };
}

const openAdvanced = (user: ReturnType<typeof userEvent.setup>) =>
  user.click(screen.getByRole('button', { name: /^advanced/i }));

describe('AgentConfigForm', () => {
  it('applies two credential patches fired in the same event', async () => {
    const user = userEvent.setup();
    const { last } = setup();
    await user.click(screen.getByRole('button', { name: 'Patch twice' }));
    expect(last()).toMatchObject({ localCredential: 'tok', credentialEnvVar: 'MY_KEY' });
  });

  it('passes the credential section the same onChange reference on every render', async () => {
    const user = userEvent.setup();
    seenOnChange.length = 0;
    setup();
    await user.type(screen.getByLabelText('Label'), 'ab');
    expect(seenOnChange.length).toBeGreaterThan(1);
    expect(new Set(seenOnChange).size).toBe(1);
  });

  it('labels the radio groups and row lists as groups', async () => {
    const user = userEvent.setup();
    setup();
    expect(screen.getByRole('group', { name: /^agent type/i })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: /^arguments/i })).toBeInTheDocument();
    await openAdvanced(user);
    expect(screen.getByRole('group', { name: /^reasoning/i })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: /^environment variables/i })).toBeInTheDocument();
  });

  it('renders the Agent, Credential and Advanced sections in order', () => {
    setup();
    const agent = screen.getByRole('heading', { name: 'Agent' });
    const credentialHeading = screen.getByRole('heading', { name: 'Credential' });
    const section = screen.getByTestId('credential-section');
    const advanced = screen.getByRole('button', { name: /^advanced/i });
    const follows = (a: Node, b: Node) =>
      Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);
    expect(follows(agent, credentialHeading)).toBe(true);
    expect(follows(credentialHeading, section)).toBe(true);
    expect(follows(section, advanced)).toBe(true);
  });

  it('shows the hint of the chosen kind and updates it when the kind changes', async () => {
    const user = userEvent.setup();
    setup();
    expect(screen.getByText(KIND_PRESETS.claude.hint)).toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
    expect(screen.getByText(KIND_PRESETS.gemini.hint)).toBeInTheDocument();
    expect(screen.queryByText(KIND_PRESETS.claude.hint)).not.toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: /^generic/i }));
    expect(screen.getByText(KIND_PRESETS.generic.hint)).toBeInTheDocument();
    expect(screen.getByTestId('credential-section')).toHaveAttribute('data-kind', 'generic');
  });

  it('explains the label and the command', () => {
    setup();
    expect(screen.getByLabelText('Label')).toBeInTheDocument();
    expect(screen.getByLabelText('Command')).toHaveValue('claude-agent-acp');
    expect(screen.getByText(/The program Rocket starts/)).toBeInTheDocument();
  });

  it('shows one argument per row and never splits on a comma', async () => {
    const user = userEvent.setup();
    const { last } = setup({ args: [newRow('', '--a'), newRow('', '--flag=a,b')] });
    expect(screen.getByLabelText('Argument 1')).toHaveValue('--a');
    expect(screen.getByLabelText('Argument 2')).toHaveValue('--flag=a,b');
    await user.click(screen.getByRole('button', { name: 'Add argument' }));
    await user.type(screen.getByLabelText('Argument 3'), 'x,y');
    expect(last().args.map((row) => row.value)).toEqual(['--a', '--flag=a,b', 'x,y']);
  });

  it('does not render the working directory and carries it through', async () => {
    const user = userEvent.setup();
    const { last } = setup({ workingDir: '/work' });
    expect(screen.queryByLabelText(/working director/i)).not.toBeInTheDocument();
    await user.type(screen.getByLabelText('Label'), 'Claude');
    expect(last().workingDir).toBe('/work');
    expect(last().label).toBe('Claude');
  });

  it('keeps a typed command when the kind changes on a new agent', async () => {
    const user = userEvent.setup();
    const { last } = setup({ command: '/opt/gemini' });
    await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
    expect(screen.getByLabelText('Command')).toHaveValue('/opt/gemini');
    expect(last().args.map((row) => row.value)).toEqual(['--acp']);
    expect(screen.getByLabelText('Argument 1')).toHaveValue('--acp');
  });

  it('shows the idle timeout summary on the collapsed Advanced section', () => {
    setup();
    expect(
      screen.getByRole('button', { name: /idle timeout 120 s, reasoning high/i }),
    ).toBeVisible();
  });

  it('shows the stored idle timeout and reasoning in the summary', () => {
    setup({ idleTimeoutSecs: '600', reasoning: 'low' });
    expect(
      screen.getByRole('button', { name: /idle timeout 600 s, reasoning low/i }),
    ).toBeVisible();
  });

  it('keeps a stored reasoning level untouched through unrelated edits', async () => {
    const user = userEvent.setup();
    const { last } = setup({ reasoning: 'low' });
    await user.type(screen.getByLabelText('Label'), 'x');
    expect(last().reasoning).toBe('low');
  });

  it('omits the reasoning from the summary for a generic agent', () => {
    setup({ kind: 'generic' });
    const trigger = screen.getByRole('button', { name: /^advanced/i });
    expect(trigger).toHaveTextContent(/idle timeout 120 s/i);
    expect(trigger).not.toHaveTextContent(/reasoning/i);
  });

  it('offers reasoning for Claude, reasoning and a model for Gemini, neither for Generic', async () => {
    const user = userEvent.setup();
    setup();
    await openAdvanced(user);
    expect(screen.getByLabelText(/idle timeout/i)).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /^high/i })).toBeChecked();
    expect(screen.queryByLabelText(/gemini model/i)).not.toBeInTheDocument();

    await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
    expect(screen.getByRole('radio', { name: /^high/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/gemini model/i)).toBeInTheDocument();

    await user.click(screen.getByRole('radio', { name: /^generic/i }));
    expect(screen.queryByRole('radio', { name: /^high/i })).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/gemini model/i)).not.toBeInTheDocument();
    expect(screen.getByLabelText(/idle timeout/i)).toBeInTheDocument();
  });

  it('writes the idle timeout, reasoning and model into the state', async () => {
    const user = userEvent.setup();
    const { last } = setup({ kind: 'gemini' });
    await openAdvanced(user);
    await user.type(screen.getByLabelText(/idle timeout/i), '300');
    await user.click(screen.getByRole('radio', { name: /^medium/i }));
    await user.type(screen.getByLabelText(/gemini model/i), 'gemini-2.5-pro');
    expect(last()).toMatchObject({
      idleTimeoutSecs: '300',
      reasoning: 'medium',
      model: 'gemini-2.5-pro',
    });
  });

  it('refuses an idle timeout outside 30 to 1800 with an inline alert', async () => {
    const user = userEvent.setup();
    setup();
    await openAdvanced(user);
    await user.type(screen.getByLabelText(/idle timeout/i), '5');
    expect(screen.getByRole('alert')).toHaveTextContent(/between 30 and 1800/i);
  });

  it('refuses a Gemini model name with spaces', async () => {
    const user = userEvent.setup();
    setup({ kind: 'gemini' });
    await openAdvanced(user);
    await user.type(screen.getByLabelText(/gemini model/i), 'gemini 2.5');
    expect(screen.getByRole('alert')).toHaveTextContent(/letters, digits/i);
  });

  it('adds extra variables as name and value rows', async () => {
    const user = userEvent.setup();
    const { last } = setup();
    await openAdvanced(user);
    await user.click(screen.getByRole('button', { name: 'Add variable' }));
    await user.type(screen.getByLabelText('Variable name 1'), 'HTTPS_PROXY');
    await user.type(screen.getByLabelText('Variable value 1'), 'http://p:8080');
    expect(last().extraEnv).toMatchObject([{ name: 'HTTPS_PROXY', value: 'http://p:8080' }]);
  });

  it('shows the base URL warning only when a variable name ends in _URL', async () => {
    const user = userEvent.setup();
    setup();
    await openAdvanced(user);
    expect(screen.queryByText(/can send the credential to another host/i)).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Add variable' }));
    await user.type(screen.getByLabelText('Variable name 1'), 'ANTHROPIC_BASE_URL');
    expect(screen.getByText(/can send the credential to another host/i)).toBeInTheDocument();
  });

  it('opens the Advanced section when it holds an error and names a Gemini reserved variable', () => {
    setup({ kind: 'gemini', extraEnv: [newRow('GEMINI_CLI_HOME', 'x')] });
    expect(screen.getByRole('alert')).toHaveTextContent(/reserved/i);
    expect(screen.getByRole('button', { name: /^advanced/i })).toHaveTextContent(
      /needs attention/i,
    );
  });

  it('opens a closed Advanced section when a kind switch creates an error inside it', async () => {
    const user = userEvent.setup();
    setup({ extraEnv: [newRow('GEMINI_CLI_HOME', 'x')] });
    const trigger = screen.getByRole('button', { name: /^advanced/i });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(trigger).not.toHaveTextContent(/needs attention/i);
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(trigger).toHaveTextContent(/needs attention/i);
    expect(screen.getByRole('alert')).toHaveTextContent(/reserved/i);
  });

  it('does not show an error for a blank variable row that was just added', async () => {
    const user = userEvent.setup();
    setup();
    await openAdvanced(user);
    await user.click(screen.getByRole('button', { name: 'Add variable' }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    await user.type(screen.getByLabelText('Variable value 1'), 'x');
    expect(screen.getByRole('alert')).toHaveTextContent('Name is required.');
  });

  it('removes an argument row with its named remove button', async () => {
    const user = userEvent.setup();
    const { last } = setup({ args: [newRow('', '--a'), newRow('', '--b')] });
    await user.click(screen.getByRole('button', { name: 'Remove argument 1' }));
    expect(last().args.map((row) => row.value)).toEqual(['--b']);
  });

  it('passes a Gemini reserved key name to the credential section as an error', () => {
    setup({ kind: 'gemini', credentialEnvVar: 'GEMINI_CLI_HOME' });
    expect(screen.getByTestId('credential-section')).toHaveTextContent(/reserved/i);
  });

  it('keeps a value swapped in from outside when the credential section patches in its effect', async () => {
    const user = userEvent.setup();
    const states: AgentFormState[] = [];
    const oldAgent = { ...emptyAgentForm('old'), label: 'Old agent' };
    const newAgent: AgentFormState = {
      ...emptyAgentForm('new'),
      isNew: false,
      label: 'New agent',
      credentialSource: 'env',
      credentialEnvVar: 'NEW_KEY',
      sourceEnvVar: '',
    };
    function Swapper() {
      const [value, setValue] = useState(oldAgent);
      return (
        <>
          <button type='button' onClick={() => setValue(newAgent)}>
            Swap agent
          </button>
          <AgentConfigForm
            value={value}
            onChange={(next) => {
              setValue(next);
              states.push(next);
            }}
            hasSavedCredential={false}
          />
        </>
      );
    }
    render(<Swapper />);
    await user.click(screen.getByRole('button', { name: 'Swap agent' }));
    expect(states).toHaveLength(1);
    expect(states[0]).toMatchObject({
      id: 'new',
      label: 'New agent',
      credentialEnvVar: 'NEW_KEY',
      sourceEnvVar: 'NEW_KEY',
    });
    expect(screen.getByLabelText('Label')).toHaveValue('New agent');
  });
});
