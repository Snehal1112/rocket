import { Copy, Download } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { copyTextAsync } from '@/lib/clipboard';
import { buildRunReport, exportableFlow, maskFlowSecrets, reportFileName } from '@/lib/flow-export';
import { type FileFilter, saveTextFile } from '@/lib/save-file';
import type { FlowTab } from '@/types/pane-types';

const FILTERS: Record<'json' | 'md', FileFilter[]> = {
  json: [{ name: 'JSON', extensions: ['json'] }],
  md: [{ name: 'Markdown', extensions: ['md'] }],
};

// Copies the flow definition, and saves the last run as a report. Credentials are
// masked in every output, and the exporter never reads the in-memory auth store.
export function FlowExportMenu({ tab }: { tab: FlowTab }) {
  const reportReady = tab.runState === 'done' && tab.nodeDetail !== undefined;

  const copyFlow = () => {
    const flow = exportableFlow(tab);
    if (!flow) return;
    const { flow: masked, maskedCount } = maskFlowSecrets(flow);
    // The clipboard write must start inside the click, so it gets a promise now.
    copyTextAsync(Promise.resolve(JSON.stringify(masked, null, 2))).then(
      () =>
        toast.success(
          maskedCount > 0
            ? `Flow JSON copied. ${maskedCount} secret${maskedCount === 1 ? '' : 's'} masked.`
            : 'Flow JSON copied.',
        ),
      (err) => toast.error(`Could not copy the flow: ${String(err)}`),
    );
  };

  const exportReport = async (format: 'json' | 'md') => {
    const report = buildRunReport(tab, { includeBodies: false });
    const text = format === 'json' ? report.json : report.markdown;
    try {
      const saved = await saveTextFile(
        reportFileName(tab.flowName ?? 'flow', format),
        text,
        FILTERS[format],
      );
      if (saved) toast.success('Run report saved.');
    } catch (err) {
      toast.error(`Could not save the report: ${String(err)}`);
    }
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size='sm' variant='outline' className='gap-1.5'>
          <Download className='h-3.5 w-3.5' aria-hidden='true' />
          Export
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align='end'>
        <DropdownMenuItem onSelect={copyFlow}>
          <Copy aria-hidden='true' />
          Copy flow JSON
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled={!reportReady} onSelect={() => void exportReport('json')}>
          Export run report (JSON)
        </DropdownMenuItem>
        <DropdownMenuItem disabled={!reportReady} onSelect={() => void exportReport('md')}>
          Export run report (Markdown)
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
