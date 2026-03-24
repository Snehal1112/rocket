import { useMemo } from 'react';
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

  if (paramNames.length === 0) return null;

  return (
    <div className="space-y-2">
      <div className="text-sm font-medium text-muted-foreground">Path Params</div>
      {paramNames.map((name) => (
        <div key={name} className="flex gap-2 items-center">
          <Input
            placeholder={`Path Key (e.g. ${name})`}
            value={name}
            readOnly
            className="flex-1 text-xs h-8 bg-muted/50"
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
        </div>
      ))}
    </div>
  );
}
