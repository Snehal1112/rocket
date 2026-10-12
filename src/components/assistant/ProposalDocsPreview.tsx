import { MarkdownRenderer } from '@/components/collections/MarkdownRenderer';

/** The proposed docs rendered as Markdown. It scrolls when the docs are long. */
export function ProposalDocsPreview({ markdown }: { markdown: string }) {
  return (
    <div className='max-h-56 overflow-auto rounded-md border p-2 text-sm'>
      <p className='mb-1 text-xs font-semibold text-muted-foreground'>Docs preview</p>
      <MarkdownRenderer restricted>{markdown}</MarkdownRenderer>
    </div>
  );
}
