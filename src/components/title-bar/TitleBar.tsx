import { type as osType } from '@tauri-apps/plugin-os';
import { WindowControls } from './WindowControls';
import { WorkspaceSwitcher } from './WorkspaceSwitcher';

export function TitleBar() {
  const isMac = osType() === 'macos';

  return (
    <div
      className='relative grid h-10 w-full grid-cols-[1fr_auto_1fr] items-center shrink-0 border-b border-titlebar-border bg-titlebar-bg text-titlebar-fg'
      data-tauri-drag-region
    >
      <div className='flex items-center h-full' data-tauri-drag-region>
        {isMac && <div className='w-[72px] shrink-0' data-tauri-drag-region />}

        <div className='flex items-center gap-2 px-3 shrink-0'>
          <img src='/rocket.png' alt='Rocket' className='h-4 w-4' />
          <span className='text-sm font-medium'>Rocket</span>
        </div>
      </div>

      <WorkspaceSwitcher />

      <div className='flex items-center justify-end h-full' data-tauri-drag-region>
        {!isMac && <WindowControls />}
      </div>
    </div>
  );
}
