import { useState, useRef, useCallback, useMemo } from 'react';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
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
  placeholder?: string;
  className?: string;
}

export function VariableAwareUrlInput({
  value,
  onChange,
  onKeyDown,
  onCurlImport,
  collectionVariables,
  pathParams,
  queryParams,
  placeholder,
  className,
}: VariableAwareUrlInputProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const environments = useEnvStore((s) => s.environments);
  const updateEnvironment = useEnvStore((s) => s.updateEnvironment);

  // Compute variables via useMemo to avoid creating a new object every render.
  // Calling getActiveVariables() inside a Zustand selector returns a fresh
  // object each time, which fails Object.is equality and causes infinite re-renders.
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

  const handleTokenHover = useCallback((token: UrlToken) => {
    setEditingToken(token);
    setEditValue(token.resolved ?? '');
  }, []);

  const handleSave = useCallback(async () => {
    if (!editingToken || !activeEnvId) return;
    const env = environments.find((e) => e.name === activeEnvId);
    if (!env) return;

    const updatedVars = env.variables.map((v) =>
      v.key === editingToken.value ? { ...v, value: editValue } : v,
    );

    // If variable doesn't exist yet, add it.
    if (!env.variables.some((v) => v.key === editingToken.value)) {
      updatedVars.push({ key: editingToken.value, value: editValue, enabled: true, secret: false });
    }

    await updateEnvironment({ ...env, variables: updatedVars });
    setEditingToken(null);
  }, [editingToken, editValue, activeEnvId, environments, updateEnvironment]);

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

            // Path param tokens: :paramName — styled span with title tooltip, no popover.
            if (token.type === 'pathParam') {
              const isResolved = token.resolved !== undefined;
              return (
                <span
                  key={i}
                  className={cn(
                    'rounded-sm px-0.5',
                    isResolved
                      ? 'bg-violet-500/15 text-violet-500'
                      : 'bg-destructive/15 text-destructive',
                  )}
                  title={isResolved ? `${token.value} = ${token.resolved}` : `${token.value} (unresolved)`}
                >
                  :{token.value}
                </span>
              );
            }

            // Query key tokens: highlighted key name.
            if (token.type === 'queryKey') {
              const isResolved = token.resolved !== undefined;
              return (
                <span
                  key={i}
                  className={cn(
                    'rounded-sm px-0.5',
                    isResolved
                      ? 'bg-amber-500/15 text-amber-500'
                      : 'text-muted-foreground',
                  )}
                  title={isResolved ? `${token.value} = ${token.resolved}` : token.value}
                >
                  {token.value}
                </span>
              );
            }

            // Query value tokens: plain muted text.
            if (token.type === 'queryValue') {
              return <span key={i} className="text-muted-foreground">{token.value}</span>;
            }

            // Variable tokens: {{name}} — with popover for editing.
            const isResolved = token.resolved !== undefined;
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
                      isResolved
                        ? 'bg-primary/15 text-primary'
                        : 'bg-destructive/15 text-destructive',
                    )}
                    onMouseEnter={() => handleTokenHover(token)}
                  >
                    {`{{${token.value}}}`}
                  </span>
                </PopoverTrigger>
                <PopoverContent className="w-72 p-3 space-y-2" side="bottom" align="start">
                  <div className="text-xs font-medium">{token.value}</div>
                  {isResolved && token.source && (
                    <div className="text-2xs text-muted-foreground">
                      Source: {token.source}
                    </div>
                  )}
                  {!isResolved && !activeEnvId && (
                    <div className="text-2xs text-destructive">
                      No active environment selected.
                    </div>
                  )}
                  {!isResolved && activeEnvId && (
                    <div className="text-2xs text-destructive">
                      Not found in {activeEnvId}.
                    </div>
                  )}
                  {activeEnvId && (
                    <div className="space-y-1.5">
                      <Input
                        className="h-7 text-xs font-mono"
                        value={editValue}
                        onChange={(e) => setEditValue(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') void handleSave();
                          if (e.key === 'Escape') setEditingToken(null);
                        }}
                        placeholder="Variable value"
                      />
                      <div className="flex gap-1.5">
                        <Button size="sm" className="h-6 text-2xs" onClick={() => void handleSave()}>
                          Save
                        </Button>
                        <Button size="sm" variant="ghost" className="h-6 text-2xs" onClick={() => setEditingToken(null)}>
                          Cancel
                        </Button>
                      </div>
                    </div>
                  )}
                </PopoverContent>
              </Popover>
            );
          })
        ) : (
          <span className="text-muted-foreground">{placeholder}</span>
        )}
      </div>
    </div>
  );
}
