interface SettingsSectionHeaderProps {
  title: string;
  description?: string;
}

// Title and optional description at the top of a Settings section.
export function SettingsSectionHeader({ title, description }: SettingsSectionHeaderProps) {
  return (
    <div className='mb-4 space-y-1'>
      <h2 className='text-base font-semibold'>{title}</h2>
      {description && <p className='text-sm text-muted-foreground'>{description}</p>}
    </div>
  );
}
