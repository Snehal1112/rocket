import { useCallback, useMemo, useRef, useState } from 'react';
import {
  type EditorToken,
  useContentEditableInput,
} from '@/hooks/useContentEditableInput';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Input } from '@/components/ui/input';
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
  type?: 'text' | 'password';
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource, key: string) => void;
}

// Navigation link label for a variable source.
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
      return null;
  }
}

export function VariableAwareInput({
  value,
  onChange,
  placeholder,
  className,
  disabled,
  type = 'text',
  variableContext,
  onNavigateToSource,
}: VariableAwareInputProps) {
  // No variableContext or password field: render a plain input.
  if (!variableContext || type === 'password') {
    return (
      <Input
        type={type}
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

// Inner component holds hooks; separated so the outer can do an early return.
function VariableAwareInputInner({
  value,
  onChange,
  placeholder,
  className,
  disabled,
  variableContext,
  onNavigateToSource,
}: Required<Pick<VariableAwareInputProps, 'variableContext'>> &
  Omit<VariableAwareInputProps, 'variableContext' | 'type'>) {
  const environments = useEnvStore((s) => s.environments);
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const updateEnvironment = useEnvStore((s) => s.updateEnvironment);
  const globalEnv = useEnvStore((s) => s.globalEnv);
  const updateGlobalEnvironment = useEnvStore((s) => s.updateGlobalEnvironment);

  const editorRef = useRef<HTMLDivElement>(null);
  // Store the element in state so the hook re-runs its effect after mount.
  const [editorEl, setEditorEl] = useState<HTMLDivElement | null>(null);

  // Index of the token whose popover is currently open.
  const [openTokenIdx, setOpenTokenIdx] = useState<number | null>(null);
  const [openVarKey, setOpenVarKey] = useState<string | null>(null);
  const [editValue, setEditValue] = useState('');
  const editingScopeRef = useRef<VariableSource | null>(null);

  // Parse value into tokens and build EditorToken list for the hook.
  const rawTokens = useMemo(() => parseTextTokens(value), [value]);

  const tokens: EditorToken[] = useMemo(
    () =>
      rawTokens.map((token, idx) => {
        if (token.type === 'text') {
          return { type: 'text' as const, content: token.content, rawLength: token.rawLength };
        }
        const entry = variableContext.get(token.content);
        const badgeClass = cn(
          'rounded-sm px-0.5 cursor-pointer',
          entry ? sourceBadgeClass(entry.source) : 'bg-destructive/15 text-destructive',
        );
        return {
          type: 'badge' as const,
          content: `{{${token.content}}}`,
          rawLength: token.rawLength,
          badgeClass,
          tokenIdx: idx,
        };
      }),
    [rawTokens, variableContext],
  );

  const { onInput, onCompositionStart, onCompositionEnd, onPaste } = useContentEditableInput({
    editorEl,
    value,
    onChange,
    tokens,
  });

  const handleBadgeMouseDown = useCallback(
    (e: React.MouseEvent) => {
      const span = (e.target as Element).closest('[data-badge]');
      if (!span) return;
      e.preventDefault(); // Prevents caret jumping into the span.
      const idx = Number(span.getAttribute('data-token-idx'));
      const rawToken = rawTokens[idx];
      if (!rawToken || rawToken.type !== 'variable') return;
      const entry = variableContext.get(rawToken.content);
      setOpenTokenIdx(idx);
      setOpenVarKey(rawToken.content);
      setEditValue(entry?.secret ? '' : (entry?.value ?? ''));
      editingScopeRef.current = entry?.source ?? null;
    },
    [rawTokens, variableContext],
  );

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
    setOpenTokenIdx(null);
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

  const handleCommitRef = useRef(handleCommit);
  handleCommitRef.current = handleCommit;

  // Badge ref map for popover anchoring — refreshed after each DOM mutation.
  const badgeRefsMap = useRef<Map<number, HTMLSpanElement>>(new Map());

  const refreshBadgeRefs = useCallback(() => {
    if (!editorRef.current) return;
    badgeRefsMap.current.clear();
    for (const span of Array.from(editorRef.current.querySelectorAll('[data-badge]'))) {
      const idx = Number((span as HTMLElement).getAttribute('data-token-idx'));
      badgeRefsMap.current.set(idx, span as HTMLSpanElement);
    }
  }, []);

  return (
    <div
      className={cn(
        'relative h-8 w-full rounded-md border border-input bg-background px-3 py-1',
        'font-mono text-xs ring-ring/50 focus-within:ring-[3px] focus-within:border-ring',
        disabled && 'opacity-50 pointer-events-none cursor-not-allowed',
        className,
      )}
      onMouseDown={handleBadgeMouseDown}
    >
      {/* Placeholder shown when value is empty. */}
      {value === '' && (
        <span
          aria-hidden
          className='absolute inset-0 flex items-center px-3 py-1 text-muted-foreground pointer-events-none'
        >
          {placeholder}
        </span>
      )}

      {/* The contenteditable editor. */}
      <div
        ref={(node) => {
          // Sync to both the mutable ref (for imperative access) and state (for hook re-run).
          (editorRef as { current: HTMLDivElement | null }).current = node;
          if (node && node !== editorEl) setEditorEl(node);
        }}
        contentEditable={!disabled}
        suppressContentEditableWarning
        role='textbox'
        aria-label={placeholder}
        aria-multiline={false}
        aria-disabled={disabled}
        spellCheck={false}
        className='outline-none h-full flex items-center'
        onInput={() => {
          onInput();
          refreshBadgeRefs();
        }}
        onCompositionStart={onCompositionStart}
        onCompositionEnd={() => {
          onCompositionEnd();
          refreshBadgeRefs();
        }}
        onPaste={(e) => onPaste(e.nativeEvent as ClipboardEvent)}
      />

      {/* Popovers rendered as siblings, outside the contenteditable div. */}
      {(() => {
        let charOffset = 0;
        return rawTokens.map((token, idx) => {
        const tokenStart = charOffset;
        charOffset += token.rawLength;
        if (token.type !== 'variable') return null;
        const entry = variableContext.get(token.content);
        const isReadOnly =
          entry !== undefined && entry.source !== 'environment' && entry.source !== 'global';
        const linkLabel = entry ? navLinkLabel(entry.source) : null;

        return (
          <Popover
            key={`${token.content}-${tokenStart}`}
            open={openTokenIdx === idx}
            onOpenChange={(open) => {
              if (!open) setOpenTokenIdx(null);
            }}
          >
            <PopoverTrigger asChild>
              <span style={{ display: 'none' }} />
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
                    if (e.key === 'Escape') {
                      setOpenTokenIdx(null);
                      setOpenVarKey(null);
                    }
                  }}
                  onBlur={() => void handleCommitRef.current()}
                />
              </div>
              {(entry || linkLabel) && (
                <div className='flex items-center justify-between px-2 py-1.5 border-t border-border/50 bg-muted/30'>
                  {entry ? (
                    <div className='flex items-center gap-1.5 text-2xs text-muted-foreground'>
                      <span
                        className={cn(
                          'rounded-full w-4 h-4 inline-flex items-center justify-center text-2xs font-bold',
                          sourceBadgeClass(entry.source),
                        )}
                      >
                        {entry.source.charAt(0).toUpperCase()}
                      </span>
                      <span>{entry.label}</span>
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
              )}
            </PopoverContent>
          </Popover>
        );
        });
      })()}
    </div>
  );
}
