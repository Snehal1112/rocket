import { useCallback, useLayoutEffect, useRef } from 'react';
import { FormField } from '@/components/ui/form-field';
import { Input } from '@/components/ui/input';
import { KeyValueRows } from '@/components/ui/key-value-rows';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { AgentAdvancedSection } from './AgentAdvancedSection';
import { AgentCredentialSection } from './AgentCredentialSection';
import { type AgentFormState, validateAgentForm, withKind } from './agent-config-form-state';
import { KIND_PRESETS } from './agent-config-validation';
import type { AgentKindValue, CredentialFormState } from './form-types';

const KIND_OPTIONS: Array<{ value: AgentKindValue; label: string }> = [
  { value: 'claude', label: 'Claude Code' },
  { value: 'gemini', label: 'Gemini CLI' },
  { value: 'generic', label: 'Generic ACP agent' },
];

interface AgentConfigFormProps {
  value: AgentFormState;
  onChange: (next: AgentFormState) => void;
  hasSavedCredential: boolean;
}

export function AgentConfigForm({ value, onChange, hasSavedCredential }: AgentConfigFormProps) {
  const errors = validateAgentForm(value);

  // Latest state and callback live in refs, so `patch` is stable and never reads a stale
  // `value`. `patch` also updates the ref at once, so two patches in one event both apply.
  const latest = useRef(value);
  const onChangeRef = useRef(onChange);
  // A layout effect runs before any child passive effect, so a child that patches in its own
  // effect after a value swap never sees the previous value.
  useLayoutEffect(() => {
    latest.current = value;
    onChangeRef.current = onChange;
  });

  const commit = useCallback((next: AgentFormState) => {
    latest.current = next;
    onChangeRef.current(next);
  }, []);
  const patch = useCallback(
    (changes: Partial<AgentFormState>) => commit({ ...latest.current, ...changes }),
    [commit],
  );
  const patchCredential = useCallback(
    (changes: Partial<CredentialFormState>) => patch(changes),
    [patch],
  );

  return (
    <div className='space-y-6'>
      <section aria-labelledby='ac-section-agent' className='space-y-4'>
        <h3 id='ac-section-agent' className='text-sm font-semibold'>
          Agent
        </h3>
        <FormField
          id='ac-label'
          label='Label'
          help='The name shown in the agent picker. Example: Claude Code (work).'
        >
          {(a11y) => (
            <Input
              {...a11y}
              value={value.label}
              onChange={(e) => patch({ label: e.target.value })}
              className='h-8 text-sm'
              autoComplete='off'
            />
          )}
        </FormField>

        <FormField id='ac-kind' label='Agent type' group help={KIND_PRESETS[value.kind].hint}>
          {() => (
            <RadioGroup
              aria-label='Agent type'
              value={value.kind}
              onValueChange={(next) => commit(withKind(latest.current, next as AgentKindValue))}
              className='gap-2'
            >
              {KIND_OPTIONS.map((option) => (
                <div key={option.value} className='flex items-center gap-2'>
                  <RadioGroupItem value={option.value} id={`ac-kind-${option.value}`} />
                  <Label htmlFor={`ac-kind-${option.value}`} className='text-sm font-normal'>
                    {option.label}
                  </Label>
                </div>
              ))}
            </RadioGroup>
          )}
        </FormField>

        <FormField
          id='ac-command'
          label='Command'
          help='The program Rocket starts for this agent, found on the PATH or given as a full path. Example: claude-agent-acp. If it is wrong, the Test button cannot start the agent.'
        >
          {(a11y) => (
            <Input
              {...a11y}
              value={value.command}
              onChange={(e) => patch({ command: e.target.value })}
              placeholder={KIND_PRESETS[value.kind].command || 'my-agent-acp'}
              className='h-8 text-sm'
              autoComplete='off'
              spellCheck={false}
            />
          )}
        </FormField>

        <FormField
          id='ac-args'
          label='Arguments'
          optionalHint='optional'
          group
          help='One argument per row, passed to the command in order. Example: --acp. A value with a comma stays one argument.'
        >
          {() => (
            <KeyValueRows
              rows={value.args}
              onChange={(rows) => patch({ args: rows })}
              columns='single'
              valueLabel='Argument'
              addLabel='Add argument'
            />
          )}
        </FormField>
      </section>

      <section aria-labelledby='ac-section-credential' className='space-y-4'>
        <h3 id='ac-section-credential' className='text-sm font-semibold'>
          Credential
        </h3>
        <AgentCredentialSection
          kind={value.kind}
          value={value}
          onChange={patchCredential}
          errors={{
            credentialEnvVar: errors.credentialEnvVar,
            sourceEnvVar: errors.sourceEnvVar,
          }}
          hasSavedCredential={hasSavedCredential}
        />
      </section>

      <AgentAdvancedSection
        kind={value.kind}
        value={value}
        errors={{ idle: errors.idle, model: errors.model, extraEnv: errors.extraEnv }}
        onChange={patch}
      />
    </div>
  );
}
