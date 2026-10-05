import { SingleLineEditor } from '@/components/editor';
import { Card, CardContent } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';

// RSA-* methods are not offered: the backend fails a request that asks for one.
const SIGNATURE_METHODS = ['HMAC-SHA1', 'HMAC-SHA256', 'HMAC-SHA512', 'PLAINTEXT'];
const PLACEMENTS = [
  { value: 'header', label: 'Authorization header' },
  { value: 'query', label: 'Query parameters' },
  { value: 'body', label: 'Form body' },
];

const TEXT_FIELDS: { key: string; label: string; secret?: boolean }[] = [
  { key: 'consumerKey', label: 'Consumer key' },
  { key: 'consumerSecret', label: 'Consumer secret', secret: true },
  { key: 'accessToken', label: 'Access token' },
  { key: 'accessTokenSecret', label: 'Access token secret', secret: true },
  { key: 'realm', label: 'Realm' },
  { key: 'callbackUrl', label: 'Callback URL' },
  { key: 'verifier', label: 'Verifier' },
];

interface OAuth1AuthEditorProps {
  // The persisted OAuth 1.0 fields as stored. Fields this editor does not show are kept.
  value: Record<string, unknown>;
  onChange: (value: Record<string, unknown>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

export function OAuth1AuthEditor({
  value,
  onChange,
  variableContext,
  onNavigateToSource,
}: OAuth1AuthEditorProps) {
  // An emptied field is removed, so the file never holds an empty string where the
  // spec has an optional value.
  const setField = (key: string, next: unknown) => {
    const merged = { ...value, [key]: next };
    if (next === '' || next === undefined) delete merged[key];
    onChange(merged);
  };
  const text = (key: string) => (typeof value[key] === 'string' ? (value[key] as string) : '');
  const method = text('signatureMethod') || 'HMAC-SHA1';
  const methodOptions = SIGNATURE_METHODS.includes(method)
    ? SIGNATURE_METHODS
    : [...SIGNATURE_METHODS, method];
  const placement = text('placement') || 'header';

  return (
    <Card>
      <CardContent className='space-y-3 p-4'>
        {TEXT_FIELDS.map((f) => (
          <div key={f.key} className='space-y-1.5'>
            <Label className='text-xs text-muted-foreground'>{f.label}</Label>
            <SingleLineEditor
              aria-label={f.label}
              placeholder={f.label}
              isSecret={f.secret}
              className='text-sm'
              value={text(f.key)}
              onChange={(next) => setField(f.key, next)}
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
          </div>
        ))}
        <div className='grid grid-cols-2 gap-2'>
          <div className='space-y-1.5'>
            <Label className='text-xs text-muted-foreground'>Signature method</Label>
            <Select value={method} onValueChange={(next) => setField('signatureMethod', next)}>
              <SelectTrigger aria-label='Signature method' className='h-8 text-xs'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {methodOptions.map((m) => (
                  <SelectItem key={m} value={m} className='text-sm'>
                    {m}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className='space-y-1.5'>
            <Label className='text-xs text-muted-foreground'>Add signature to</Label>
            <Select value={placement} onValueChange={(next) => setField('placement', next)}>
              <SelectTrigger aria-label='Signature placement' className='h-8 text-xs'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {PLACEMENTS.map((p) => (
                  <SelectItem key={p.value} value={p.value} className='text-sm'>
                    {p.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
        <div className='flex items-center gap-2'>
          <Checkbox
            checked={value.includeBodyHash === true}
            onCheckedChange={(checked) => setField('includeBodyHash', checked === true)}
            aria-label='Include body hash'
          />
          <span className='text-xs text-muted-foreground'>Include body hash</span>
        </div>
      </CardContent>
    </Card>
  );
}
