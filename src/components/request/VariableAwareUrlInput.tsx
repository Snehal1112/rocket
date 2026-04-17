import { useCallback, useMemo, useRef, useState } from 'react';
import {
  type EditorToken,
  useContentEditableInput,
} from '@/hooks/useContentEditableInput';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { type ParsedCurl, parseCurl } from '@/lib/curl-parser';
import {
  parseUrlTokens,
  sourceBadgeClass,
  type UrlToken,
  type VariableScopeEntry,
  type VariableSource,
} from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';

interface VariableAwareUrlInputProps {
  value: string;
  onChange: (value: string) => void;
  onKeyDown?: React.KeyboardEventHandler<HTMLDivElement>;
  onCurlImport?: (parsed: ParsedCurl) => void;
  collectionVariables?: Record<string, string>;
  pathParams?: Record<string, string>;
  queryParams?: Record<string, string>;
  onPathParamChange?: (key: string, value: string) => void;
  onNavigateToSource?: (source: VariableSource | 'pathParam') => void;
  placeholder?: string;
  className?: string;
  scopedContext?: Map<string, VariableScopeEntry>;
}

// Returns the link label for a navigation destination, or null if no nav is available.
function navLinkLabel(source: VariableSource | 'pathParam'): string | null {
  switch (source) {
    case 'pathParam':
      return 'Params \u2192';
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

export function VariableAwareUrlInput({
  value,
  onChange,
  onKeyDown,
  onCurlImport,
  collectionVariables,
  pathParams,
  queryParams,
  onPathParamChange,
  onNavigateToSource,
  placeholder,
  className,
  scopedContext,
}: VariableAwareUrlInputProps) {
  const [editorEl, setEditorEl] = useState<HTMLDivElement | null>(null);
  const editorCallbackRef = useCallback((node: HTMLDivElement | null) => {
    setEditorEl(node);
  }, []);

  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const environments = useEnvStore((s) => s.environments);
  const updateEnvironment = useEnvStore((s) => s.updateEnvironment);
  const globalEnv = useEnvStore((s) => s.globalEnv);
  const updateGlobalEnvironment = useEnvStore((s) => s.updateGlobalEnvironment);

  const [editingToken, setEditingToken] = useState<UrlToken | null>(null);
  const [editValue, setEditValue] = useState('');
  // Tracks the variable scope of the token being edited so handleCommit saves to the right place.
  const editingScopeRef = useRef<VariableSource | null>(null);

  const variables = useMemo(() => {
    if (!activeEnvId) return {};
    const env = environments.find((e) => e.name === activeEnvId);
    if (!env) return {};
    const vars: Record<string, string> = {};
    for (const v of env.variables) {
      if (v.enabled) vars[v.key] = v.value;
    }
    return vars;
  }, [activeEnvId, environments]);

  const urlTokens = useMemo(
    () =>
      parseUrlTokens(
        value,
        variables,
        activeEnvId ?? undefined,
        collectionVariables,
        pathParams,
        queryParams,
      ),
    [value, variables, activeEnvId, collectionVariables, pathParams, queryParams],
  );

  // Map UrlTokens to EditorTokens for the hook.
  const editorTokens: EditorToken[] = useMemo(
    () =>
      urlTokens.map((token, idx) => {
        if (token.type === 'text') {
          return { type: 'text' as const, content: token.value, rawLength: token.value.length };
        }
        if (token.type === 'queryValue') {
          return {
            type: 'badge' as const,
            content: token.value,
            rawLength: token.value.length,
            badgeClass: 'text-muted-foreground',
            tokenIdx: idx,
          };
        }
        if (token.type === 'queryKey') {
          const isResolved = token.resolved !== undefined;
          return {
            type: 'badge' as const,
            content: token.value,
            rawLength: token.value.length,
            badgeClass: cn(
              'rounded-sm px-0.5',
              isResolved ? 'bg-amber-500/15 text-amber-500' : 'text-muted-foreground',
            ),
            tokenIdx: idx,
          };
        }
        if (token.type === 'pathParam') {
          const isResolved = token.resolved !== undefined;
          return {
            type: 'badge' as const,
            content: `:${token.value}`,
            rawLength: token.value.length + 1,
            badgeClass: cn(
              'rounded-sm px-0.5 cursor-pointer',
              isResolved
                ? 'bg-violet-500/15 text-violet-500'
                : 'bg-destructive/15 text-destructive',
            ),
            tokenIdx: idx,
          };
        }
        // Variable token.
        const scopeEntry = scopedContext?.get(token.value);
        const badgeClass = cn(
          'rounded-sm px-0.5 cursor-pointer',
          scopeEntry
            ? sourceBadgeClass(scopeEntry.source)
            : token.resolved !== undefined
              ? 'bg-primary/15 text-primary'
              : 'bg-destructive/15 text-destructive',
        );
        return {
          type: 'badge' as const,
          content: `{{${token.value}}}`,
          rawLength: token.value.length + 4,
          badgeClass,
          tokenIdx: idx,
        };
      }),
    [urlTokens, scopedContext],
  );

  // cURL paste interception — must run before the hook's default paste.
  const handleBeforePaste = useCallback(
    (e: ClipboardEvent): boolean => {
      if (!onCurlImport) return false;
      const text = e.clipboardData?.getData('text/plain').trim() ?? '';
      if (!/^curl\s/i.test(text)) return false;
      e.preventDefault();
      const parsed = parseCurl(text);
      if (parsed) onCurlImport(parsed);
      return true;
    },
    [onCurlImport],
  );

  const { onInput, onCompositionStart, onCompositionEnd, onPaste } = useContentEditableInput({
    editorEl,
    value,
    onChange,
    tokens: editorTokens,
    onBeforePaste: handleBeforePaste,
  });

  const badgeRefsMap = useRef<Map<number, HTMLSpanElement>>(new Map());

  const refreshBadgeRefs = useCallback(() => {
    if (!editorEl) return;
    badgeRefsMap.current.clear();
    for (const span of Array.from(editorEl.querySelectorAll('[data-badge]'))) {
      const idx = Number((span as HTMLElement).getAttribute('data-token-idx'));
      badgeRefsMap.current.set(idx, span as HTMLSpanElement);
    }
  }, [editorEl]);

  const handleBadgeMouseDown = useCallback(
    (e: React.MouseEvent) => {
      const span = (e.target as Element).closest('[data-badge]');
      if (!span) return;
      const idx = Number(span.getAttribute('data-token-idx'));
      const token = urlTokens[idx];
      if (!token || (token.type !== 'variable' && token.type !== 'pathParam')) return;
      e.preventDefault();
      const scopeEntry = token.type === 'variable' ? scopedContext?.get(token.value) : undefined;
      setEditingToken(token);
      setEditValue(scopeEntry?.secret ? '' : (scopeEntry?.value ?? token.resolved ?? ''));
      editingScopeRef.current = scopeEntry?.source ?? null;
    },
    [urlTokens, scopedContext],
  );

  // Save the edited value based on token type.
  const handleCommit = useCallback(async () => {
    if (!editingToken) return;
    if (editingToken.type === 'pathParam' && onPathParamChange) {
      onPathParamChange(editingToken.value, editValue);
    } else if (editingToken.type === 'variable') {
      const scope = editingScopeRef.current;
      if (scope === 'global' && globalEnv) {
        const vars = globalEnv.variables.map((v) =>
          v.key === editingToken.value ? { ...v, value: editValue } : v,
        );
        if (!globalEnv.variables.some((v) => v.key === editingToken.value)) {
          vars.push({
            key: editingToken.value,
            value: editValue,
            enabled: true,
            secret: false,
          });
        }
        await updateGlobalEnvironment({ ...globalEnv, variables: vars });
      } else if ((scope === 'environment' || scope === null) && activeEnvId) {
        const env = environments.find((e) => e.name === activeEnvId);
        if (env) {
          const vars = env.variables.map((v) =>
            v.key === editingToken.value ? { ...v, value: editValue } : v,
          );
          if (!env.variables.some((v) => v.key === editingToken.value)) {
            vars.push({
              key: editingToken.value,
              value: editValue,
              enabled: true,
              secret: false,
            });
          }
          await updateEnvironment({ ...env, variables: vars });
        }
      }
    }
    setEditingToken(null);
  }, [
    editingToken,
    editValue,
    activeEnvId,
    environments,
    updateEnvironment,
    globalEnv,
    updateGlobalEnvironment,
    onPathParamChange,
  ]);

  const handleCommitRef = useRef(handleCommit);
  handleCommitRef.current = handleCommit;

  return (
    <div
      className={cn('relative flex-1 h-8', className)}
      onMouseDown={handleBadgeMouseDown}
    >
      {value === '' && (
        <span
          aria-hidden
          className='absolute inset-0 flex items-center px-3 py-1 font-mono text-xs text-muted-foreground pointer-events-none'
        >
          {placeholder}
        </span>
      )}

      <div
        ref={editorCallbackRef}
        contentEditable
        suppressContentEditableWarning
        role='textbox'
        aria-label={placeholder}
        aria-multiline={false}
        spellCheck={false}
        className='h-full w-full rounded-md border border-input bg-background px-3 py-1 font-mono text-xs outline-none ring-ring/50 focus-visible:ring-[3px] focus-visible:border-ring flex items-center'
        onInput={() => {
          onInput();
          refreshBadgeRefs();
        }}
        onKeyDown={onKeyDown}
        onCompositionStart={onCompositionStart}
        onCompositionEnd={() => {
          onCompositionEnd();
          refreshBadgeRefs();
        }}
        onPaste={(e) => onPaste(e.nativeEvent as ClipboardEvent)}
      />

      {/* Popovers for interactive tokens (variable, pathParam). */}
      {urlTokens.map((token) => {
        if (token.type !== 'variable' && token.type !== 'pathParam') return null;
        const scopeEntry = token.type === 'variable' ? scopedContext?.get(token.value) : undefined;
        const isReadOnlyVar =
          token.type === 'variable' &&
          scopeEntry !== undefined &&
          scopeEntry.source !== 'environment' &&
          scopeEntry.source !== 'global';
        const navSource: VariableSource | 'pathParam' | null =
          token.type === 'pathParam' ? 'pathParam' : (scopeEntry?.source ?? null);
        const linkLabel = navSource !== null ? navLinkLabel(navSource) : null;

        return (
          <Popover
            key={token.start}
            open={editingToken?.start === token.start}
            onOpenChange={(open) => {
              if (!open) setEditingToken(null);
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
                  value={scopeEntry?.secret ? '●●●●' : editValue}
                  onChange={(e) => {
                    if (scopeEntry?.secret || isReadOnlyVar) return;
                    const v = e.target.value;
                    setEditValue(v);
                    if (token.type === 'pathParam' && onPathParamChange) {
                      onPathParamChange(token.value, v);
                    }
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') void handleCommitRef.current();
                    if (e.key === 'Escape') setEditingToken(null);
                  }}
                  onBlur={() => void handleCommitRef.current()}
                  placeholder='Value'
                  readOnly={isReadOnlyVar || scopeEntry?.secret}
                />
              </div>
              <div className='flex items-center justify-between px-2 py-1.5 border-t border-border/50 bg-muted/30'>
                <div className='flex items-center gap-1.5 text-2xs text-muted-foreground'>
                  {token.type === 'pathParam' ? (
                    <span className='text-violet-500 font-bold text-xs'>:</span>
                  ) : scopeEntry ? (
                    <span
                      className={cn(
                        'rounded-full w-4 h-4 inline-flex items-center justify-center text-2xs font-bold',
                        sourceBadgeClass(scopeEntry.source),
                      )}
                    >
                      {scopeEntry.source.charAt(0).toUpperCase()}
                    </span>
                  ) : null}
                  <span>
                    {scopeEntry?.label ??
                      (token.type === 'pathParam' ? 'Path Variable' : 'Unresolved')}
                  </span>
                </div>
                {onNavigateToSource && navSource !== null && linkLabel !== null && (
                  <button
                    type='button'
                    className='text-2xs text-primary hover:underline cursor-pointer'
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={async () => {
                      await handleCommitRef.current();
                      onNavigateToSource(navSource);
                    }}
                  >
                    {linkLabel}
                  </button>
                )}
              </div>
            </PopoverContent>
          </Popover>
        );
      })}
    </div>
  );
}
