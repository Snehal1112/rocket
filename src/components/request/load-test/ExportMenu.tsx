import { Download } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import type { ExportFormat } from '@/lib/tauri-api';
import { useLoadTestStore } from '@/stores/load-test-store';

const FORMATS: { value: ExportFormat; label: string }[] = [
  { value: 'html', label: 'HTML report' },
  { value: 'csv', label: 'CSV (request log)' },
  { value: 'json', label: 'JSON (full snapshot)' },
  { value: 'pdf', label: 'PDF summary' },
];

export function ExportMenu() {
  const exportResult = useLoadTestStore((s) => s.exportResult);
  const status = useLoadTestStore((s) => s.status);
  const disabled = status !== 'complete';

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant='outline' size='sm' className='w-full text-xs' disabled={disabled}>
          <Download className='mr-1 h-3 w-3' />
          Export
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align='end'>
        {FORMATS.map((f) => (
          <DropdownMenuItem
            key={f.value}
            className='text-xs'
            onSelect={() => exportResult(f.value)}
          >
            {f.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
