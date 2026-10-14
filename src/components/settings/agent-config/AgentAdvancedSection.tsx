import { useEffect, useState } from 'react';
import { CollapsibleSection } from '@/components/ui/collapsible-section';
import { FormField } from '@/components/ui/form-field';
import { Input } from '@/components/ui/input';
import { KeyValueRows } from '@/components/ui/key-value-rows';
import { Label } from '@/components/ui/label';
import { NumberInput } from '@/components/ui/number-input';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import type { ReasoningLevel } from '@/lib/tauri-api';
import type { AdvancedFields, AgentFormErrors } from './agent-config-form-state';
import { MAX_IDLE_SECONDS, MIN_IDLE_SECONDS } from './agent-config-validation';
import type { AgentKindValue } from './form-types';

const DEFAULT_IDLE_SECONDS = 120;

const REASONING_LEVELS: Array<{ value: ReasoningLevel; label: string; hint: string }> = [
  { value: 'low', label: 'Low', hint: 'Fast answers, little deliberation.' },
  { value: 'medium', label: 'Medium', hint: 'Balanced.' },
  { value: 'high', label: 'High', hint: 'Deepest reasoning. Best for guiding and correcting.' },
];

interface AgentAdvancedSectionProps {
  kind: AgentKindValue;
  value: AdvancedFields;
  errors: Pick<AgentFormErrors, 'idle' | 'model' | 'extraEnv'>;
  onChange: (patch: Partial<AdvancedFields>) => void;
}

export function AgentAdvancedSection({ kind, value, errors, onChange }: AgentAdvancedSectionProps) {
  const hasReasoning = kind === 'claude' || kind === 'gemini';
  const hasError =
    errors.idle !== null || errors.model !== null || errors.extraEnv.some((e) => e !== null);
  const idleText = value.idleTimeoutSecs.trim() || String(DEFAULT_IDLE_SECONDS);
  const baseSummary = `Idle timeout ${idleText} s${hasReasoning ? `, reasoning ${value.reasoning}` : ''}`;
  const summary = hasError ? `${baseSummary} (needs attention)` : baseSummary;
  // Controlled so an error inside the section can never stay hidden.
  const [open, setOpen] = useState(hasError);
  useEffect(() => {
    if (hasError) setOpen(true);
  }, [hasError]);
  const hasUrlVariable = value.extraEnv.some((row) =>
    row.name.trim().toUpperCase().endsWith('_URL'),
  );

  return (
    <CollapsibleSection title='Advanced' summary={summary} open={open} onOpenChange={setOpen}>
      <div className='space-y-4'>
        <FormField
          id='ac-idle-timeout'
          label='Idle timeout'
          optionalHint='seconds, optional'
          help={`How long a turn may stay silent before the session ends. Leave it empty for ${DEFAULT_IDLE_SECONDS} seconds, or raise it (up to ${MAX_IDLE_SECONDS}) for deep reasoning on a slow model.`}
          error={errors.idle}
        >
          {(a11y) => (
            <NumberInput
              {...a11y}
              min={MIN_IDLE_SECONDS}
              max={MAX_IDLE_SECONDS}
              value={value.idleTimeoutSecs}
              onChange={(e) => onChange({ idleTimeoutSecs: e.target.value })}
              placeholder={String(DEFAULT_IDLE_SECONDS)}
              className='h-8 text-sm'
              containerClassName='w-full'
            />
          )}
        </FormField>

        {hasReasoning && (
          <FormField
            id='ac-reasoning'
            label='Reasoning'
            group
            help="Claude uses its effort setting, Gemini its thinking budget. You can still change Claude's effort in the chat toolbar."
          >
            {() => (
              <RadioGroup
                aria-label='Reasoning'
                value={value.reasoning}
                onValueChange={(next) => onChange({ reasoning: next as ReasoningLevel })}
                className='gap-2'
              >
                {REASONING_LEVELS.map((level) => (
                  <div key={level.value} className='flex items-center gap-2'>
                    <RadioGroupItem value={level.value} id={`ac-reasoning-${level.value}`} />
                    <Label htmlFor={`ac-reasoning-${level.value}`} className='text-sm font-normal'>
                      {level.label} <span className='text-muted-foreground'>({level.hint})</span>
                    </Label>
                  </div>
                ))}
              </RadioGroup>
            )}
          </FormField>
        )}

        {kind === 'gemini' && (
          <FormField
            id='ac-gemini-model'
            label='Gemini model'
            optionalHint='optional'
            help='Leave empty to let Gemini choose. Example: gemini-2.5-pro. A name Gemini does not know makes the session fail to start.'
            error={errors.model}
          >
            {(a11y) => (
              <Input
                {...a11y}
                value={value.model}
                onChange={(e) => onChange({ model: e.target.value })}
                placeholder='auto'
                className='h-8 text-sm'
                autoComplete='off'
                spellCheck={false}
              />
            )}
          </FormField>
        )}

        <FormField
          id='ac-extra-env'
          label='Environment variables'
          optionalHint='optional'
          group
          help='Extra variables the agent process receives. Values are stored as plain text, so put secrets in the credential source, not here.'
        >
          {() => (
            <div className='space-y-2'>
              <KeyValueRows
                rows={value.extraEnv}
                onChange={(rows) => onChange({ extraEnv: rows })}
                columns='pair'
                nameLabel='Variable name'
                valueLabel='Variable value'
                addLabel='Add variable'
                rowErrors={errors.extraEnv}
              />
              {hasUrlVariable && (
                <p role='note' className='text-xs text-muted-foreground'>
                  A variable such as ANTHROPIC_BASE_URL can send the credential to another host.
                  Only set it to a server you trust.
                </p>
              )}
            </div>
          )}
        </FormField>
      </div>
    </CollapsibleSection>
  );
}
