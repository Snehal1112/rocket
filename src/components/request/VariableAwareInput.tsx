import { useCallback, useRef, useState } from 'react';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { parseTextTokens } from '@/lib/text-variables';
import {
  sourceBadgeClass,
  type VariableScopeEntry,
  type VariableSource,
} from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';

export interface VariableAwareInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  className?: string;
  disabled?: boolean;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource, key: string) => void;
}

// Human-readable label and badge icon for a resolved scope entry.
function sourceMeta(entry: VariableScopeEntry) {
  return {
    icon: entry.source.charAt(0).toUpperCase(),
    iconClass: cn(
      'rounded-full w-4 h-4 inline-flex items-center justify-center text-2xs font-bold',
      sourceBadgeClass(entry.source),
    ),
    label: entry.label,
  };
}

// Navigation link label for a given variable source, or null when no nav is available.
function navLinkLabel(source: VariableSource): string | null {
  switch (source) {
    case 'request':
    case 'runtime':
      return 'Request Variables \u2192';
    case 'environment':
      return 'Collection Environments \u2192';
    case 'global':
      return 'Global Environments \u2192';
    case 'collection':
      return 'Collection Variables \u2192';
    default:
      return null; // folder, process — no navigation available
  }
}

export function VariableAwareInput({
  value,
  onChange,
  placeholder,
  className,
  disabled,
  variableContext,
  onNavigateToSource,
}: VariableAwareInputProps) {
  // No variableContext: render a plain Input to avoid unnecessary overhead.
  if (!variableContext) {
    return (
      <Input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className={cn('text-xs', className)}
        disabled={disabled}
      />
    );
  }

  return (
    <VariableAwareInputInner
      value={value}
      onChange={onChange}
      placeholder={placeholder}
      className={className}
      disabled={disabled}
      variableContext={variableContext}
      onNavigateToSource={onNavigateToSource}
    />
  );
}

// Inner component holds all hooks, separated so the outer component can do an early return.
function VariableAwareInputInner({
  value,
  onChange,
  placeholder,
  className,
  disabled,
  variableContext,
  onNavigateToSource,
}: Required<Pick<VariableAwareInputProps, 'variableContext'>> &
  Omit<VariableAwareInputProps, 'variableContext'>) {
  const environments = useEnvStore((s) => s.environments);
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const updateEnvironment = useEnvStore((s) => s.updateEnvironment);
  const globalEnv = useEnvStore((s) => s.globalEnv);
  const updateGlobalEnvironment = useEnvStore((s) => s.updateGlobalEnvironment);

  // Key of the variable whose popover is currently open.
  const [openVarKey, setOpenVarKey] = useState<string | null>(null);
  const [editValue, setEditValue] = useState('');
  // Tracks the scope of the variable being edited so handleCommit saves to the right store.
  const editingScopeRef = useRef<VariableSource | null>(null);

  const tokens = parseTextTokens(value);

  const handleTokenHover = useCallback((varKey: string, entry: VariableScopeEntry | undefined) => {
    setOpenVarKey(varKey);
    setEditValue(entry?.secret ? '' : (entry?.value ?? ''));
    editingScopeRef.current = entry?.source ?? null;
  }, []);

  // Persist the edited value to the appropriate environment store.
  const handleCommit = useCallback(async () => {
    if (!openVarKey) return;
    const scope = editingScopeRef.current;

    if (scope === 'global' && globalEnv) {
      const vars = globalEnv.variables.map((v) =>
        v.key === openVarKey ? { ...v, value: editValue } : v,
      );
      if (!globalEnv.variables.some((v) => v.key === openVarKey)) {
        vars.push({ key: openVarKey, value: editValue, enabled: true, secret: false });
      }
      await updateGlobalEnvironment({ ...globalEnv, variables: vars });
    } else if ((scope === 'environment' || scope === null) && activeEnvId) {
      const env = environments.find((e) => e.name === activeEnvId);
      if (env) {
        const vars = env.variables.map((v) =>
          v.key === openVarKey ? { ...v, value: editValue } : v,
        );
        if (!env.variables.some((v) => v.key === openVarKey)) {
          vars.push({ key: openVarKey, value: editValue, enabled: true, secret: false });
        }
        await updateEnvironment({ ...env, variables: vars });
      }
    }

    setOpenVarKey(null);
  }, [
    openVarKey,
    editValue,
    activeEnvId,
    environments,
    updateEnvironment,
    globalEnv,
    updateGlobalEnvironment,
  ]);

  // Stable ref so inline event handlers always call the latest version.
  const handleCommitRef = useRef(handleCommit);
  handleCommitRef.current = handleCommit;

  return (
    <div className={cn('relative', className)}>
      {/* Transparent real input receives keystrokes and shows the text caret. */}
      <input
        type='text'
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        disabled={disabled}
        className={cn(
          'h-8 w-full rounded-md border border-input bg-background px-3 py-1 font-mono text-xs',
          'text-transparent caret-foreground outline-none ring-ring/50 pointer-events-auto',
          'focus-visible:ring-[3px] focus-visible:border-ring',
          'disabled:cursor-not-allowed disabled:opacity-50',
        )}
      />

      {/* Overlay renders token highlights; pointer events disabled except on variable buttons. */}
      <div
        className='absolute inset-0 flex items-center px-3 py-1 font-mono text-xs pointer-events-none overflow-hidden whitespace-nowrap'
        aria-hidden='true'
      >
        {tokens.length > 0 ? (
          tokens.map((token, idx) => {
            if (token.type === 'text') {
              // biome-ignore lint/suspicious/noArrayIndexKey: tokens have no stable id
              return <span key={idx}>{token.content}</span>;
            }

            const entry = variableContext.get(token.content);
            const badgeClass = entry
              ? sourceBadgeClass(entry.source)
              : 'bg-destructive/15 text-destructive';

            // Only environment and global vars can be edited from this popover.
            const isReadOnly =
              entry !== undefined && entry.source !== 'environment' && entry.source !== 'global';

            const linkLabel = entry ? navLinkLabel(entry.source) : null;
            const meta = entry ? sourceMeta(entry) : null;

            return (
              <Popover
                // biome-ignore lint/suspicious/noArrayIndexKey: tokens have no stable id
                key={idx}
                open={openVarKey === token.content}
                onOpenChange={(open) => {
                  if (!open) setOpenVarKey(null);
                }}
              >
                <PopoverTrigger asChild>
                  <button
                    type='button'
                    className={cn(
                      'rounded-sm px-0.5 cursor-pointer pointer-events-auto bg-transparent border-0',
                      badgeClass,
                    )}
                    onMouseEnter={() => handleTokenHover(token.content, entry)}
                    onClick={() => handleTokenHover(token.content, entry)}
                  >
                    {`{{${token.content}}}`}
                  </button>
                </PopoverTrigger>
                <PopoverContent className='w-80 p-0' side='bottom' align='start'>
                  <div className='p-2'>
                    <Input
                      autoFocus
                      className='h-7 text-xs font-mono'
                      value={entry?.secret ? '●●●●' : editValue}
                      placeholder={entry ? 'Value' : 'Not set'}
                      readOnly={isReadOnly || entry?.secret}
                      onChange={(e) => {
                        if (isReadOnly || entry?.secret) return;
                        setEditValue(e.target.value);
                      }}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') void handleCommitRef.current();
                        if (e.key === 'Escape') setOpenVarKey(null);
                      }}
                      onBlur={() => void handleCommitRef.current()}
                    />
                  </div>
                  <div className='flex items-center justify-between px-2 py-1.5 border-t border-border/50 bg-muted/30'>
                    {meta ? (
                      <div className='flex items-center gap-1.5 text-2xs text-muted-foreground'>
                        <span className={meta.iconClass}>{meta.icon}</span>
                        <span>{meta.label}</span>
                      </div>
                    ) : (
                      <div className='text-2xs text-muted-foreground'>Unresolved</div>
                    )}
                    {onNavigateToSource && entry && linkLabel && (
                      <button
                        type='button'
                        className='text-2xs text-primary hover:underline cursor-pointer'
                        onMouseDown={(e) => e.preventDefault()}
                        onClick={async () => {
                          await handleCommitRef.current();
                          onNavigateToSource(entry.source, token.content);
                        }}
                      >
                        {linkLabel}
                      </button>
                    )}
                  </div>
                </PopoverContent>
              </Popover>
            );
          })
        ) : (
          <span className='text-muted-foreground'>{placeholder}</span>
        )}
      </div>
    </div>
  );
}
