import { ChevronDown, Send, Square } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { ConfigOption } from '@/lib/tauri-api';
import { formatUsage, type UsageInput } from './usage-format';

export type AssistantModeValue = 'ask' | 'edit' | 'agent';

export const MODEL_OPTION_ID = 'model';
export const EFFORT_OPTION_ID = 'effort';

export const MODE_OPTIONS: readonly {
  value: AssistantModeValue;
  label: string;
  description: string;
}[] = [
  { value: 'ask', label: 'Ask', description: 'Reads the workspace and answers.' },
  { value: 'edit', label: 'Edit', description: 'Also proposes changes for you to accept.' },
  {
    value: 'agent',
    label: 'Agent',
    description:
      "Also runs requests in collections that allow it, with the environment's credentials only.",
  },
];

interface PickerChoice {
  value: string;
  label: string;
  description?: string | null;
}

interface PickerProps {
  title: string;
  value: string;
  choices: readonly PickerChoice[];
  onChange(value: string): void;
  disabled?: boolean;
  /** Why the picker is disabled, shown on hover. */
  disabledReason?: string;
}

function Picker({ title, value, choices, onChange, disabled, disabledReason }: PickerProps) {
  const shown = choices.find((choice) => choice.value === value)?.label ?? value;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant='ghost'
          size='sm'
          className='h-7 gap-1 px-2 text-xs'
          aria-label={`${title}: ${shown}`}
          title={disabled ? disabledReason : undefined}
          disabled={disabled}
        >
          <span className='max-w-32 truncate'>{shown}</span>
          <ChevronDown className='size-3 opacity-60' />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align='start' side='top'>
        <DropdownMenuLabel className='text-xs'>{title}</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={value} onValueChange={onChange}>
          {choices.map((choice) => (
            <DropdownMenuRadioItem
              key={choice.value}
              value={choice.value}
              className='flex-col items-start'
            >
              <span>{choice.label}</span>
              {choice.description ? (
                <span className='text-xs text-muted-foreground'>{choice.description}</span>
              ) : null}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function toChoices(option: ConfigOption): PickerChoice[] {
  return option.choices.map((choice) => ({
    value: choice.value,
    label: choice.name,
    description: choice.description,
  }));
}

export interface ComposerToolbarProps {
  mode: AssistantModeValue;
  onModeChange(mode: AssistantModeValue): void;
  configOptions: readonly ConfigOption[];
  onConfigChange(configId: string, value: string): void;
  usage?: UsageInput;
  running: boolean;
  canSend: boolean;
  /** True when there is no active session. The pickers and Send are off. */
  disabled?: boolean;
  onSend(): void;
  onStop(): void;
}

const NO_SESSION_REASON = 'Start a session to use this.';
const RUNNING_REASON = 'Wait for the reply to finish.';

/** Mode, Model and Effort pickers, the context-used indicator and Send or Stop. */
export function ComposerToolbar({
  mode,
  onModeChange,
  configOptions,
  onConfigChange,
  usage,
  running,
  canSend,
  disabled,
  onSend,
  onStop,
}: ComposerToolbarProps) {
  const model = configOptions.find((option) => option.id === MODEL_OPTION_ID);
  const effort = configOptions.find((option) => option.id === EFFORT_OPTION_ID);
  const usageView = formatUsage(usage);
  const pickersDisabled = disabled || running;
  const reason = disabled ? NO_SESSION_REASON : RUNNING_REASON;
  const modeSummary = MODE_OPTIONS.find((option) => option.value === mode)?.description;

  return (
    <div className='border-t'>
      <div className='flex items-center gap-1 px-1.5 py-1'>
        <Picker
          title='Mode'
          value={mode}
          choices={MODE_OPTIONS}
          disabled={pickersDisabled}
          disabledReason={reason}
          onChange={(value) => {
            const next = MODE_OPTIONS.find((option) => option.value === value);
            if (next) onModeChange(next.value);
          }}
        />
        {model && model.choices.length > 0 ? (
          <Picker
            title='Model'
            value={model.currentValue}
            choices={toChoices(model)}
            disabled={pickersDisabled}
            disabledReason={reason}
            onChange={(value) => onConfigChange(model.id, value)}
          />
        ) : null}
        {effort && effort.choices.length > 0 ? (
          <Picker
            title='Effort'
            value={effort.currentValue}
            choices={toChoices(effort)}
            disabled={pickersDisabled}
            disabledReason={reason}
            onChange={(value) => onConfigChange(effort.id, value)}
          />
        ) : null}
        <div className='flex-1' />
        {usageView ? (
          <TooltipProvider delayDuration={200}>
            <Tooltip>
              <TooltipTrigger asChild>
                <span className='px-1 text-xs tabular-nums text-muted-foreground'>
                  <span>{usageView.text}</span>
                  <span className='sr-only'>{` context used, ${usageView.detail}`}</span>
                </span>
              </TooltipTrigger>
              <TooltipContent>{usageView.detail}</TooltipContent>
            </Tooltip>
          </TooltipProvider>
        ) : null}
        {running ? (
          <Button
            variant='secondary'
            size='icon'
            className='size-7'
            aria-label='Stop'
            onClick={onStop}
          >
            <Square className='size-3.5' />
          </Button>
        ) : (
          <Button
            size='icon'
            className='size-7'
            aria-label='Send'
            title={disabled ? NO_SESSION_REASON : undefined}
            disabled={!canSend}
            onClick={onSend}
          >
            <Send className='size-3.5' />
          </Button>
        )}
      </div>
      <p className='px-2.5 pb-1.5 text-xs text-muted-foreground'>
        {modeSummary} A mode change applies from the next tool call.
      </p>
    </div>
  );
}
