interface SettingsSectionHeaderProps {
  title: string;
  description?: string;
}

// Title and optional description at the top of a Settings section.
export function SettingsSectionHeader({ title, description }: SettingsSectionHeaderProps) {
  return (
    <div className='mb-6 space-y-1 border-b border-border pb-4'>
      <h2 className='text-lg font-semibold tracking-tight'>{title}</h2>
      {description && <p className='text-sm text-muted-foreground'>{description}</p>}
    </div>
  );
}
