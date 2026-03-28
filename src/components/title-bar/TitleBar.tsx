import { useEffect, useState } from 'react'
import { type } from '@tauri-apps/plugin-os'
import { WorkspaceSwitcher } from './WorkspaceSwitcher'
import { WindowControls } from './WindowControls'

export function TitleBar() {
  const [platform, setPlatform] = useState<string>('linux')

  useEffect(() => {
    type().then(setPlatform)
  }, [])

  const isMac = platform === 'macos'

  return (
    <div
      className="flex h-11 w-full items-center shrink-0 border-b bg-background"
      data-tauri-drag-region
    >
      {isMac && <div className="w-[72px] shrink-0" data-tauri-drag-region />}

      <div className="flex items-center gap-2 px-3 shrink-0">
        <img src="/rocket.png" alt="Rocket" className="h-4 w-4" />
        <span className="text-sm font-medium">Rocket</span>
      </div>

      <div className="flex-1" data-tauri-drag-region />

      <WorkspaceSwitcher />

      <div className="flex-1" data-tauri-drag-region />

      {!isMac && <WindowControls />}
    </div>
  )
}
