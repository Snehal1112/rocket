import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { ComplianceProfile, EnforcementLevel, Framework } from '@/lib/tauri-api';
import { useAuditStore } from '@/stores/audit-store';

interface ComplianceProfileDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const FRAMEWORKS: { value: Framework; label: string; hint: string }[] = [
  { value: 'soc2', label: 'SOC 2 Type 2', hint: 'Trust Services Criteria' },
  { value: 'iso27001', label: 'ISO 27001:2022', hint: 'Annex A controls' },
  { value: 'iso42001', label: 'ISO 42001:2023', hint: 'AI management' },
  { value: 'csaStar', label: 'CSA STAR', hint: 'Cloud Controls Matrix' },
];

const ENFORCEMENT: { value: EnforcementLevel; label: string; description: string }[] = [
  { value: 'record', label: 'Record', description: 'Log events without interfering' },
  { value: 'warn', label: 'Warn', description: 'Log and surface a toast on sensitive ops' },
  { value: 'block', label: 'Block', description: 'Log and reject the triggering operation' },
];

export function ComplianceProfileDialog({ open, onOpenChange }: ComplianceProfileDialogProps) {
  const profile = useAuditStore((s) => s.profile);
  const loadProfile = useAuditStore((s) => s.loadProfile);
  const saveProfile = useAuditStore((s) => s.saveProfile);

  const [draft, setDraft] = useState<ComplianceProfile>({
    activeFrameworks: [],
    enforcement: 'record',
    mutedKinds: [],
  });

  useEffect(() => {
    if (open) void loadProfile();
  }, [open, loadProfile]);

  useEffect(() => {
    if (profile) setDraft(profile);
  }, [profile]);

  const toggleFramework = (fw: Framework) => {
    setDraft((d) => ({
      ...d,
      activeFrameworks: d.activeFrameworks.includes(fw)
        ? d.activeFrameworks.filter((x) => x !== fw)
        : [...d.activeFrameworks, fw],
    }));
  };

  const handleSave = async () => {
    await saveProfile(draft);
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-w-lg'>
        <DialogHeader>
          <DialogTitle>Compliance Profile</DialogTitle>
        </DialogHeader>

        <div className='space-y-5 py-2'>
          <div className='space-y-2'>
            <Label className='text-sm font-medium'>Active frameworks</Label>
            <p className='text-xs text-muted-foreground'>
              Events are recorded for every kind tagged against a selected framework. With no
              frameworks selected, all events are recorded.
            </p>
            <div className='grid grid-cols-1 gap-2'>
              {FRAMEWORKS.map((f) => (
                <label
                  key={f.value}
                  className='flex cursor-pointer items-start gap-2.5 rounded-md border border-border px-3 py-2 hover:bg-muted/30'
                >
                  <Checkbox
                    checked={draft.activeFrameworks.includes(f.value)}
                    onCheckedChange={() => toggleFramework(f.value)}
                    className='mt-0.5'
                  />
                  <div className='flex-1'>
                    <div className='text-sm font-medium'>{f.label}</div>
                    <div className='text-xs text-muted-foreground'>{f.hint}</div>
                  </div>
                </label>
              ))}
            </div>
          </div>

          <div className='space-y-1.5'>
            <Label htmlFor='enforcement' className='text-sm font-medium'>
              Enforcement level
            </Label>
            <Select
              value={draft.enforcement}
              onValueChange={(v) => setDraft((d) => ({ ...d, enforcement: v as EnforcementLevel }))}
            >
              <SelectTrigger id='enforcement'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ENFORCEMENT.map((e) => (
                  <SelectItem key={e.value} value={e.value}>
                    <div>
                      <div className='font-medium'>{e.label}</div>
                      <div className='text-xs text-muted-foreground'>{e.description}</div>
                    </div>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <DialogFooter>
          <Button variant='ghost' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={() => void handleSave()}>Save</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
