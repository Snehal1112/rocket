import { open } from '@tauri-apps/plugin-dialog';
import { FolderOpen, Loader2, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  type GrpcExecuteInput,
  type GrpcMethodInfo,
  type GrpcMethodType,
  type GrpcServiceInfo,
  grpcListServices,
} from '@/lib/tauri-api';
import type { VariableScopeEntry } from '@/lib/url-variables';

export const METHOD_TYPE_LABEL: Record<GrpcMethodType, string> = {
  unary: 'Unary',
  'client-streaming': 'Client stream',
  'server-streaming': 'Server stream',
  'bidi-streaming': 'Bidi stream',
};

interface GrpcMethodPickerProps {
  /** `package.Service/Method`, empty until one is picked. */
  method: string;
  /** The call shape stored with the request. A loaded list corrects it when it disagrees. */
  methodType: GrpcMethodType;
  protoFilePath: string;
  onProtoFilePathChange: (path: string) => void;
  onPick: (method: GrpcMethodInfo) => void;
  /** Builds the call input when the methods are listed, so it always reads the latest edits. */
  buildInput: () => GrpcExecuteInput;
  /** Changes whenever the proto path or the URL changes, so a stale list is dropped. */
  sourceKey: string;
  variableContext?: Map<string, VariableScopeEntry>;
}

/**
 * Picks the method to call. The list comes from the request's `.proto` file or, when no
 * file is set, from server reflection on the URL.
 */
export function GrpcMethodPicker({
  method,
  methodType,
  protoFilePath,
  onProtoFilePathChange,
  onPick,
  buildInput,
  sourceKey,
  variableContext,
}: GrpcMethodPickerProps) {
  const [services, setServices] = useState<GrpcServiceInfo[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  // Each source gets its own generation. A list that was asked for before the file or URL
  // changed must not land on the new source, whenever it arrives.
  const generation = useRef(0);

  // A different file or URL means the loaded list may no longer apply.
  // biome-ignore lint/correctness/useExhaustiveDependencies: sourceKey is the trigger on purpose.
  useEffect(() => {
    generation.current += 1;
    setServices(null);
    setError('');
    setLoading(false);
  }, [sourceKey]);

  const load = useCallback(
    async (refresh: boolean) => {
      const mine = generation.current;
      setLoading(true);
      setError('');
      try {
        const list = await grpcListServices(buildInput(), refresh);
        if (generation.current !== mine) return;
        setServices(list);
        // The server knows the real call shape. Selecting the method that is already
        // chosen does not fire the select's change event, so fix a wrong stored shape here.
        const listed = list.flatMap((s) => s.methods).find((x) => x.fullName === method);
        if (listed && listed.methodType !== methodType) onPick(listed);
      } catch (err) {
        if (generation.current !== mine) return;
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        if (generation.current === mine) setLoading(false);
      }
    },
    [buildInput, method, methodType, onPick],
  );

  const handleBrowse = useCallback(async () => {
    const picked = await open({
      multiple: false,
      title: 'Select a .proto file',
      filters: [{ name: 'Protocol Buffers', extensions: ['proto'] }],
    });
    if (typeof picked === 'string') onProtoFilePathChange(picked);
  }, [onProtoFilePathChange]);

  const handleValueChange = (fullName: string) => {
    const found = services?.flatMap((s) => s.methods).find((m) => m.fullName === fullName);
    if (found) onPick(found);
  };

  return (
    <div className='flex flex-col gap-2'>
      <div className='flex items-center gap-2'>
        <div className='flex-1 min-w-0'>
          <SingleLineEditor
            aria-label='Proto file path'
            placeholder='Path to a .proto file, or leave empty to use server reflection'
            value={protoFilePath}
            onChange={onProtoFilePathChange}
            variableContext={variableContext}
          />
        </div>
        <Button
          type='button'
          size='sm'
          variant='outline'
          className='h-8 px-2'
          aria-label='Browse for a proto file'
          onClick={() => void handleBrowse()}
        >
          <FolderOpen className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
      </div>
      <div className='flex items-center gap-2'>
        <Select
          value={method}
          onValueChange={handleValueChange}
          onOpenChange={(isOpen) => {
            if (isOpen && services === null && !loading) void load(false);
          }}
        >
          <SelectTrigger className='h-8 flex-1 font-mono text-xs' aria-label='Method'>
            <SelectValue placeholder='Choose a method'>{method || undefined}</SelectValue>
          </SelectTrigger>
          <SelectContent>
            {loading && (
              <div className='flex items-center gap-2 px-2 py-1.5 text-xs text-muted-foreground'>
                <Loader2 className='h-3 w-3 animate-spin' aria-hidden='true' /> Loading methods
              </div>
            )}
            {(services ?? []).map((service) => (
              <SelectGroup key={service.name}>
                <SelectLabel className='font-mono text-xs'>{service.name}</SelectLabel>
                {service.methods.map((m) => (
                  <SelectItem key={m.fullName} value={m.fullName}>
                    <span className='flex items-center gap-2'>
                      <span className='font-mono text-xs'>{m.name}</span>
                      <Badge variant='outline' className='text-[10px] px-1.5 py-0'>
                        {METHOD_TYPE_LABEL[m.methodType]}
                      </Badge>
                    </span>
                  </SelectItem>
                ))}
              </SelectGroup>
            ))}
            {services !== null && services.length === 0 && !loading && (
              <div className='px-2 py-1.5 text-xs text-muted-foreground'>No services found</div>
            )}
          </SelectContent>
        </Select>
        <Button
          type='button'
          size='sm'
          variant='outline'
          className='h-8 px-2'
          aria-label='Reload methods'
          disabled={loading}
          onClick={() => void load(true)}
        >
          <RefreshCw className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
      </div>
      {error && (
        <p role='alert' className='text-xs text-destructive'>
          {error}
        </p>
      )}
      {!protoFilePath.trim() && !error && (
        <p className='text-xs text-muted-foreground'>
          No .proto file set. Methods come from server reflection on the URL.
        </p>
      )}
    </div>
  );
}
