import { openUrl } from '@tauri-apps/plugin-opener';
import { Bug, ExternalLink, GitFork, type LucideIcon } from 'lucide-react';
import { toast } from 'sonner';
import { SettingsSectionHeader } from '@/components/settings/SettingsSectionHeader';
import { Button } from '@/components/ui/button';

export const SUPPORT_LINKS: readonly { label: string; url: string; icon: LucideIcon }[] = [
  { label: 'Report issues', url: 'https://github.com/Snehal1112/rocket/issues', icon: Bug },
  { label: 'GitHub', url: 'https://github.com/Snehal1112/rocket', icon: GitFork },
];

async function openLink(url: string) {
  try {
    await openUrl(url);
  } catch {
    // The URL is in the message so the user can copy it.
    toast.error(`Could not open the browser. Go to ${url}`);
  }
}

export function SupportSection() {
  return (
    <div>
      <SettingsSectionHeader title='Support' description='Get help and report problems.' />
      <div className='flex flex-col items-start gap-2'>
        {SUPPORT_LINKS.map(({ label, url, icon: Icon }) => (
          <Button
            key={url}
            variant='link'
            className='h-auto gap-2 p-0'
            onClick={() => void openLink(url)}
          >
            <Icon className='h-4 w-4' aria-hidden='true' />
            {label}
            <ExternalLink className='h-3 w-3' aria-hidden='true' />
          </Button>
        ))}
      </div>
    </div>
  );
}
