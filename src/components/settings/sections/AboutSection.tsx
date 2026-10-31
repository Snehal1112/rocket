import { getVersion } from '@tauri-apps/api/app';
import { arch, platform, version } from '@tauri-apps/plugin-os';
import { useEffect, useState } from 'react';
import { SettingsSectionHeader } from '@/components/settings/SettingsSectionHeader';
import { getAppDataDir } from '@/lib/tauri-api';

const UNKNOWN = 'Unknown';

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
  ];

  return (
    <div>
      <SettingsSectionHeader title='About' description='Rocket, an API client.' />
      <dl className='grid grid-cols-[8rem_1fr] gap-x-4 gap-y-2 text-sm'>
        {rows.map(([label, value]) => (
          <div key={label} className='contents'>
            <dt className='text-muted-foreground'>{label}</dt>
            <dd className='select-text break-all'>{value ?? '…'}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
