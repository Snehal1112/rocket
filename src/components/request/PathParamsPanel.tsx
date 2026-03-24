import { useMemo } from 'react';
import { Check, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { extractPathParams } from '@/lib/url-params';

interface PathParamsPanelProps {
  url: string;
  onUrlChange: (newUrl: string) => void;
}

function replacePathParam(url: string, name: string, value: string): string {
  let result = url.replace(
    new RegExp(`:${name}(?=[/?.&#]|$)`),
    value || `:${name}`,
  );
  result = result.replace(
    new RegExp(`\\{${name}\\}`),
    value || `{${name}}`,
  );
  return result;
}

export function PathParamsPanel({ url, onUrlChange }: PathParamsPanelProps) {
  const paramNames = useMemo(() => extractPathParams(url), [url]);

  return (
    <>
      <div className="text-sm font-medium text-muted-foreground">Path Params</div>
      {paramNames.length === 0 ? (
        <p className="text-xs text-muted-foreground/70">
          Add <code className="rounded bg-muted px-1 py-0.5 text-[11px]">:param</code> or <code className="rounded bg-muted px-1 py-0.5 text-[11px]">{'{param}'}</code> in the URL to see path parameters.
        </p>
      ) : (
        paramNames.map((name) => (
          <div key={name} className="flex gap-2 items-center">
            <Button
              variant="ghost"
              size="icon"
              className="w-4 h-4 rounded border p-0 bg-primary border-primary text-primary-foreground hover:bg-primary/90"
              aria-label={`Path param ${name} enabled`}
            >
              <Check className="h-3 w-3" />
            </Button>
            <Input
              placeholder={`Path Key (e.g. ${name})`}
              value={name}
              readOnly
              className="flex-1 text-xs h-8"
            />
            <Input
              placeholder="Value"
              className="flex-1 text-xs h-8"
              onChange={(e) => {
                const newUrl = replacePathParam(url, name, e.target.value);
                onUrlChange(newUrl);
              }}
              aria-label={`Value for path parameter ${name}`}
            />
            <Button variant="ghost" size="icon" className="h-7 w-7 invisible">
              <X className="h-4 w-4" />
            </Button>
          </div>
        ))
      )}
    </>
  );
}
