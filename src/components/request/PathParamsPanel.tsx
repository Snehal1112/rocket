import { useMemo } from 'react';
import { Input } from '@/components/ui/input';
import { extractPathParams } from '@/lib/url-params';

interface PathParamsPanelProps {
  url: string;
  onUrlChange: (newUrl: string) => void;
}

// Replaces a single path parameter placeholder with the given value.
function replacePathParam(
  url: string,
  name: string,
  value: string,
): string {
  // Replace :name pattern.
  let result = url.replace(
    new RegExp(`:${name}(?=[/?.&#]|$)`),
    value || `:${name}`,
  );
  // Replace {name} pattern.
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
    <div className="space-y-1">
      <h4 className="text-xs font-medium text-muted-foreground">
        Path Parameters
      </h4>
      <div className="grid grid-cols-[8rem_1fr] gap-1">
        {paramNames.map((name) => (
          <div
            key={name}
            className="col-span-2 grid grid-cols-subgrid items-center gap-1"
          >
            <span className="truncate rounded bg-muted px-2 py-1 text-xs font-mono">
              {name}
            </span>
            <Input
              className="h-7 text-xs"
              placeholder={`value for :${name}`}
              onChange={(e) => {
                const newUrl = replacePathParam(url, name, e.target.value);
                onUrlChange(newUrl);
              }}
              aria-label={`Value for path parameter ${name}`}
            />
          </div>
        ))}
      </div>
    </div>
  );
}
