import { useState, useRef, useCallback, useMemo } from 'react';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';
import { parseUrlTokens, type UrlToken } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';
import { parseCurl, type ParsedCurl } from '@/lib/curl-parser';

interface VariableAwareUrlInputProps {
  value: string;
  onChange: (value: string) => void;
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>;
  onCurlImport?: (parsed: ParsedCurl) => void;
  collectionVariables?: Record<string, string>;
  pathParams?: Record<string, string>;
  queryParams?: Record<string, string>;
  onPathParamChange?: (key: string, value: string) => void;
  onSwitchToParams?: () => void;
  placeholder?: string;
  className?: string;
}

// Determines the type badge and label for the popover footer.
function tokenMeta(token: UrlToken, _activeEnvId: string | null) {
  switch (token.type) {
    case 'pathParam':
      return { icon: ':', iconClass: 'text-violet-500', label: 'Path Variable' };
    case 'variable':
      if (token.source === 'Collection') {
        return { icon: 'C', iconClass: 'bg-muted-foreground text-background rounded-full w-4 h-4 inline-flex items-center justify-center text-2xs font-bold', label: 'Collection' };
      }
      return { icon: 'E', iconClass: 'bg-amber-500 text-background rounded-full w-4 h-4 inline-flex items-center justify-center text-2xs font-bold', label: 'Environments' };
    default:
      return { icon: '?', iconClass: 'text-muted-foreground', label: '' };
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
  onSwitchToParams,
  placeholder,
  className,
}: VariableAwareUrlInputProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const environments = useEnvStore((s) => s.environments);
  const updateEnvironment = useEnvStore((s) => s.updateEnvironment);

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

  const [editingToken, setEditingToken] = useState<UrlToken | null>(null);
  const [editValue, setEditValue] = useState('');

  const tokens = parseUrlTokens(value, variables, activeEnvId ?? undefined, collectionVariables, pathParams, queryParams);

  const handleTokenClick = useCallback((token: UrlToken) => {
    setEditingToken(token);
    setEditValue(token.resolved ?? '');
  }, []);

  // Save the edited value based on token type.
  const handleCommit = useCallback(async () => {
    if (!editingToken) return;

    if (editingToken.type === 'pathParam' && onPathParamChange) {
      onPathParamChange(editingToken.value, editValue);
    } else if (editingToken.type === 'variable' && editingToken.source !== 'Collection' && activeEnvId) {
      const env = environments.find((e) => e.name === activeEnvId);
      if (env) {
        const updatedVars = env.variables.map((v) =>
          v.key === editingToken.value ? { ...v, value: editValue } : v,
        );
        if (!env.variables.some((v) => v.key === editingToken.value)) {
          updatedVars.push({ key: editingToken.value, value: editValue, enabled: true, secret: false });
        }
        await updateEnvironment({ ...env, variables: updatedVars });
      }
    }

    setEditingToken(null);
  }, [editingToken, editValue, activeEnvId, environments, updateEnvironment, onPathParamChange]);

  const handlePaste = useCallback((e: React.ClipboardEvent<HTMLInputElement>) => {
    if (!onCurlImport) return;
    const text = e.clipboardData.getData('text/plain').trim();
    if (!/^curl\s/i.test(text)) return;
    e.preventDefault();
    const parsed = parseCurl(text);
    if (parsed) {
      onCurlImport(parsed);
    }
  }, [onCurlImport]);

  // Renders the unified Postman-style popover for any editable token.
  function renderTokenPopover(token: UrlToken, i: number, displayText: string, tokenColorClass: string) {
    const meta = tokenMeta(token, activeEnvId);
    const isCollectionVar = token.type === 'variable' && token.source === 'Collection';

    return (
      <Popover
        key={i}
        open={editingToken?.start === token.start}
        onOpenChange={(open) => { if (!open) setEditingToken(null); }}
      >
        <PopoverTrigger asChild>
          <span
            className={cn(
              'rounded-sm px-0.5 cursor-pointer pointer-events-auto',
              tokenColorClass,
            )}
            onMouseEnter={() => handleTokenClick(token)}
          >
            {displayText}
          </span>
        </PopoverTrigger>
        <PopoverContent className="w-80 p-0" side="bottom" align="start">
          {/* Value input. */}
          <div className="p-2">
            <Input
              autoFocus
              className="h-7 text-xs font-mono"
              value={editValue}
              onChange={(e) => {
                setEditValue(e.target.value);
                // Live update for path params.
                if (token.type === 'pathParam' && onPathParamChange) {
                  onPathParamChange(token.value, e.target.value);
                }
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void handleCommit();
                if (e.key === 'Escape') setEditingToken(null);
              }}
              onBlur={() => void handleCommit()}
              placeholder="Value"
              readOnly={isCollectionVar}
            />
          </div>

          {/* Footer: type badge (left) + "Variables in request →" link (right). */}
          <div className="flex items-center justify-between px-2 py-1.5 border-t border-border/50 bg-muted/30">
            <div className="flex items-center gap-1.5 text-2xs text-muted-foreground">
              {token.type === 'pathParam' ? (
                <span className="text-violet-500 font-bold text-xs">:</span>
              ) : meta.icon === 'E' ? (
                <span className={meta.iconClass}>{meta.icon}</span>
              ) : (
                <span className={meta.iconClass}>{meta.icon}</span>
              )}
              <span>{meta.label}</span>
            </div>
            {onSwitchToParams && (
              <button
                type="button"
                className="text-2xs text-primary hover:underline cursor-pointer"
                onClick={() => { setEditingToken(null); onSwitchToParams(); }}
              >
                Variables in request &rarr;
              </button>
            )}
          </div>
        </PopoverContent>
      </Popover>
    );
  }

  return (
    <div className={cn('relative flex-1', className)}>
      {/* Real input for keyboard interaction. */}
      <input
        ref={inputRef}
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
        onPaste={handlePaste}
        placeholder={placeholder}
        className="h-8 w-full rounded-md border border-input bg-background px-3 py-1 font-mono text-xs text-transparent caret-foreground outline-none ring-ring/50 focus-visible:ring-[3px] focus-visible:border-ring"
      />

      {/* Overlay with token highlights. */}
      <div
        className="absolute inset-0 flex items-center px-3 py-1 font-mono text-xs pointer-events-none overflow-hidden whitespace-nowrap"
        aria-hidden="true"
      >
        {tokens.length > 0 ? (
          tokens.map((token, i) => {
            if (token.type === 'text') {
              return <span key={i}>{token.value}</span>;
            }

            // Path param: unified popover.
            if (token.type === 'pathParam') {
              const isResolved = token.resolved !== undefined;
              return renderTokenPopover(
                token, i, `:${token.value}`,
                isResolved ? 'bg-violet-500/15 text-violet-500' : 'bg-destructive/15 text-destructive',
              );
            }

            // Query key: styled span (no popover — edit in Params tab).
            if (token.type === 'queryKey') {
              const isResolved = token.resolved !== undefined;
              return (
                <span
                  key={i}
                  className={cn(
                    'rounded-sm px-0.5 pointer-events-auto',
                    isResolved ? 'bg-amber-500/15 text-amber-500' : 'text-muted-foreground',
                  )}
                  title={isResolved ? `${token.value} = ${token.resolved}` : token.value}
                >
                  {token.value}
                </span>
              );
            }

            // Query value: plain muted text.
            if (token.type === 'queryValue') {
              return <span key={i} className="text-muted-foreground">{token.value}</span>;
            }

            // Variable token: unified popover.
            const isResolved = token.resolved !== undefined;
            return renderTokenPopover(
              token, i, `{{${token.value}}}`,
              isResolved ? 'bg-primary/15 text-primary' : 'bg-destructive/15 text-destructive',
            );
          })
        ) : (
          <span className="text-muted-foreground">{placeholder}</span>
        )}
      </div>
    </div>
  );
}
