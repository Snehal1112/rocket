import { useMemo } from 'react';
import { Input } from '@/components/ui/input';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
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
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead className="h-7 w-[8rem] text-xs">Name</TableHead>
            <TableHead className="h-7 text-xs">Value</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {paramNames.map((name) => (
            <TableRow key={name}>
              <TableCell className="p-1">
                <span className="truncate rounded bg-muted px-2 py-1 text-xs font-mono">
                  {name}
                </span>
              </TableCell>
              <TableCell className="p-1">
                <Input
                  className="h-7 text-xs"
                  placeholder={`value for :${name}`}
                  onChange={(e) => {
                    const newUrl = replacePathParam(url, name, e.target.value);
                    onUrlChange(newUrl);
                  }}
                  aria-label={`Value for path parameter ${name}`}
                />
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
