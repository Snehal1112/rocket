import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

// Every node kind has a label, so every editor starts with this field.
export function LabelField({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className='space-y-1'>
      <Label htmlFor='flow-node-label' className='text-xs'>
        Label
      </Label>
      <Input
        id='flow-node-label'
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className='h-8 text-xs'
      />
    </div>
  );
}
