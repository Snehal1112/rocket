import { Badge } from '@/components/ui/badge';
import { useSavedRequestPreview } from './useSavedRequestPreview';

const AUTH_LABELS: Record<string, string> = {
  none: 'None',
  inherit: 'Inherited',
  basic: 'Basic',
  bearer: 'Bearer',
  'api-key': 'API key',
  'o-auth2': 'OAuth 2.0',
  'aws-sig-v4': 'AWS Signature v4',
};

// Read-only summary of a saved request. Its content is edited in its own tab.
export function SavedRequestDetails({
  collection,
  requestPath,
}: {
  collection: string;
  requestPath: string;
}) {
  const { preview, error, loading } = useSavedRequestPreview(collection, requestPath);
  if (error) {
    return <p className='text-xs text-red-600'>Could not load request: {error}</p>;
  }
  if (loading || !preview) {
    return <p className='text-xs text-muted-foreground'>Loading request…</p>;
  }
  const headers = preview.headers.filter((h) => h.enabled);
  return (
    <div className='space-y-2 text-xs' data-testid='saved-request-details'>
      <div className='flex items-center gap-2'>
        <Badge variant='secondary' className='font-mono text-[10px]'>
          {preview.method}
        </Badge>
        <span data-testid='saved-request-url' className='truncate font-mono' title={preview.url}>
          {preview.url}
        </span>
      </div>
      <div data-testid='saved-request-headers' className='space-y-0.5'>
        <span className='font-medium'>Headers</span>
        {headers.length === 0 ? (
          <p className='text-muted-foreground'>None</p>
        ) : (
          headers.map((h) => (
            <p key={h.key} className='truncate font-mono' title={`${h.key}: ${h.value}`}>
              {h.key}: {h.value}
            </p>
          ))
        )}
      </div>
      <p data-testid='saved-request-auth'>
        <span className='font-medium'>Auth</span>{' '}
        {AUTH_LABELS[preview.authType] ?? preview.authType}
      </p>
      <div className='space-y-0.5'>
        <span className='font-medium'>Body</span>
        {preview.bodyPreview === null ? (
          <p className='text-muted-foreground'>None</p>
        ) : (
          <pre
            data-testid='saved-request-body'
            className='max-h-40 overflow-auto whitespace-pre-wrap rounded border bg-muted p-2 font-mono'
          >
            {preview.bodyPreview}
          </pre>
        )}
      </div>
    </div>
  );
}
