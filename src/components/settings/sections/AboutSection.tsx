import { getVersion } from '@tauri-apps/api/app';
import { arch, platform, version } from '@tauri-apps/plugin-os';
import { ExternalLink } from 'lucide-react';
import { useEffect, useState } from 'react';
import { SettingsSectionHeader } from '@/components/settings/SettingsSectionHeader';
import { openLink } from '@/components/settings/sections/SupportSection';
import { UpdatesBlock } from '@/components/settings/sections/UpdatesBlock';
import { Button } from '@/components/ui/button';
import { getAppDataDir } from '@/lib/tauri-api';

const UNKNOWN = 'Unknown';

// The wording follows the landing page description, without its benchmark numbers.
export const ABOUT_TEXT =
  'Rocket is a fast, native desktop API client for HTTP, GraphQL, gRPC and WebSocket. ' +
  'Collections are plain files you can keep in git. A visual Flow builder chains requests, and ' +
  'an AI assistant proposes changes for you to approve. Everything stays on your machine: no ' +
  'cloud and no account.';
export const WEBSITE_URL = 'https://web.rocket.numericlabs.io';
export const LICENSE_NAME = 'MIT';
export const COPYRIGHT = '© 2026 Snehal Dangroshiya';

function readPlatform(): string {
  try {
    return `${platform()} ${version()} (${arch()})`;
  } catch {
    return UNKNOWN;
  }
}

export function AboutSection() {
  const [appVersion, setAppVersion] = useState<string | null>(null);
  const [dataDir, setDataDir] = useState<string | null>(null);
  const [platformText] = useState(readPlatform);

  useEffect(() => {
    let disposed = false;
    // Each value falls back on its own, so one failure does not hide the others.
    getVersion().then(
      (v) => !disposed && setAppVersion(v),
      () => !disposed && setAppVersion(UNKNOWN),
    );
    getAppDataDir().then(
      (d) => !disposed && setDataDir(d),
      () => !disposed && setDataDir(UNKNOWN),
    );
    return () => {
      disposed = true;
    };
  }, []);

  const rows: [string, string | null][] = [
    ['Version', appVersion],
    ['Data folder', dataDir],
    ['Platform', platformText],
    ['License', LICENSE_NAME],
    ['Copyright', COPYRIGHT],
  ];

  return (
    <div>
      <SettingsSectionHeader title='About' description='Rocket, an API client.' />
      <p className='mb-4 max-w-prose text-sm text-muted-foreground'>{ABOUT_TEXT}</p>
      <dl className='grid grid-cols-[8rem_1fr] gap-x-4 gap-y-2 text-sm'>
        {rows.map(([label, value]) => (
          <div key={label} className='contents'>
            <dt className='text-muted-foreground'>{label}</dt>
            <dd className='select-text break-all'>{value ?? '…'}</dd>
          </div>
        ))}
        <dt className='text-muted-foreground'>Website</dt>
        <dd>
          <Button
            variant='link'
            className='h-auto gap-1 p-0 text-sm'
            onClick={() => void openLink(WEBSITE_URL)}
          >
            web.rocket.numericlabs.io
            <ExternalLink className='h-3 w-3' aria-hidden='true' />
          </Button>
        </dd>
      </dl>
      <UpdatesBlock />
    </div>
  );
}
