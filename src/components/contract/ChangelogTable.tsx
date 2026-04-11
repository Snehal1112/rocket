import { Badge } from '@/components/ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import type { ChangelogEntry } from '@/lib/tauri-api';

interface ChangelogTableProps {
  entries: ChangelogEntry[];
}

export function ChangelogTable({ entries }: ChangelogTableProps) {
  if (entries.length === 0) {
    return (
      <div className='flex items-center justify-center py-16 border rounded-lg'>
        <p className='text-sm text-muted-foreground'>
          No changes recorded since contract was signed.
        </p>
      </div>
    );
  }

  const badgeVariant = (changeType: ChangelogEntry['changeType']) => {
    if (changeType === 'removed') return 'destructive';
    if (changeType === 'added') return 'default';
    return 'secondary';
  };

  return (
    <div className='border rounded-lg overflow-hidden'>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead className='text-xs w-28'>Date</TableHead>
            <TableHead className='text-xs'>Field</TableHead>
            <TableHead className='text-xs w-24'>Type</TableHead>
            <TableHead className='text-xs'>Before</TableHead>
            <TableHead className='text-xs'>After</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {entries.map((entry) => (
            <TableRow key={`${entry.timestamp}:${entry.field}`}>
              {/* Date */}
              <TableCell className='text-xs text-muted-foreground whitespace-nowrap'>
                {new Date(entry.timestamp).toLocaleDateString(undefined, {
                  month: 'short',
                  day: 'numeric',
                  year: '2-digit',
                })}
              </TableCell>

              {/* Field name — mono */}
              <TableCell>
                <code className='text-xs font-mono bg-muted px-1.5 py-0.5 rounded'>
                  {entry.field}
                </code>
              </TableCell>

              {/* Change type badge */}
              <TableCell>
                <Badge variant={badgeVariant(entry.changeType)} className='text-xs capitalize'>
                  {entry.changeType}
                </Badge>
              </TableCell>

              {/* Old value */}
              <TableCell className='text-xs text-muted-foreground'>
                {entry.oldValue ? (
                  <code className='font-mono bg-muted px-1 rounded'>{entry.oldValue}</code>
                ) : (
                  <span>—</span>
                )}
              </TableCell>

              {/* New value */}
              <TableCell className='text-xs'>
                {entry.newValue ? (
                  <code className='font-mono bg-muted px-1 rounded'>{entry.newValue}</code>
                ) : (
                  <span>—</span>
                )}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
