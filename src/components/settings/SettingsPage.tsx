import { X } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { resolveSection, SETTINGS_SECTIONS } from '@/components/settings/settings-sections';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { useSettingsStore } from '@/stores/settings-store';

// Esc from these keeps Settings open, so typing and nested popups are not interrupted.
const ESC_IGNORED_FROM =
  'input, textarea, select, [contenteditable="true"], [role="combobox"], [role="listbox"], [role="menu"], [role="dialog"], [role="alertdialog"]';

export function SettingsPage() {
  const sectionId = useSettingsStore((s) => s.section);
  const locked = useSettingsStore((s) => s.locked);
  const setSection = useSettingsStore((s) => s.setSection);
  const closeSettings = useSettingsStore((s) => s.closeSettings);
  const rootRef = useRef<HTMLElement>(null);
  const active = resolveSection(sectionId);
  const ActiveSection = active.Component;

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    // Remember the focused element so focus can go back when Settings closes.
    const previous = document.activeElement;
    root.focus();
    // A native listener: Radix popups render in portals outside this subtree, so their Esc stays theirs.
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.defaultPrevented) return;
      if (e.target instanceof Element && e.target.closest(ESC_IGNORED_FROM)) return;
      e.preventDefault();
      useSettingsStore.getState().closeSettings();
    };
    root.addEventListener('keydown', onKeyDown);
    return () => {
      root.removeEventListener('keydown', onKeyDown);
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, []);

  return (
    <section
      ref={rootRef}
      tabIndex={-1}
      aria-label='Settings'
      className='flex h-full flex-col bg-background outline-none'
    >
      <header className='flex h-10 shrink-0 items-center justify-between border-b px-4'>
        <h1 className='text-sm font-semibold'>Settings</h1>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='Close settings'
          disabled={locked}
          onClick={closeSettings}
        >
          <X className='h-4 w-4' aria-hidden='true' />
        </Button>
      </header>
      <div className='flex min-h-0 flex-1'>
        <nav aria-label='Settings sections' className='w-48 shrink-0 space-y-0.5 border-r p-2'>
          {SETTINGS_SECTIONS.map((s) => {
            const Icon = s.icon;
            const isActive = s.id === active.id;
            return (
              <Button
                key={s.id}
                variant='ghost'
                size='sm'
                aria-current={isActive ? 'page' : undefined}
                disabled={locked && !isActive}
                onClick={() => setSection(s.id)}
                className={cn(
                  'w-full justify-start gap-2 font-normal',
                  isActive && 'bg-accent text-accent-foreground',
                )}
              >
                <Icon className='h-4 w-4' aria-hidden='true' />
                {s.label}
              </Button>
            );
          })}
        </nav>
        <div className='min-w-0 flex-1 overflow-y-auto'>
          <div className={cn('p-6', active.wide ? 'max-w-3xl' : 'max-w-xl')}>
            <ActiveSection key={active.id} />
          </div>
        </div>
      </div>
    </section>
  );
}
