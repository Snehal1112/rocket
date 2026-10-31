import { Info, LifeBuoy, type LucideIcon } from 'lucide-react';
import type { ComponentType } from 'react';
import { AboutSection } from '@/components/settings/sections/AboutSection';
import { SupportSection } from '@/components/settings/sections/SupportSection';
import type { SettingsSection } from '@/stores/settings-store';

export interface SettingsSectionDef {
  id: SettingsSection;
  label: string;
  icon: LucideIcon;
  Component: ComponentType;
  /** A wider panel, for sections with lists and wide forms. */
  wide?: boolean;
}

// Bruno's order. Later sections go in at their Bruno position (spec section 4.5).
export const SETTINGS_SECTIONS: SettingsSectionDef[] = [
  { id: 'support', label: 'Support', icon: LifeBuoy, Component: SupportSection },
  { id: 'about', label: 'About', icon: Info, Component: AboutSection },
];

// An unknown or missing id falls back to the first section.
export function resolveSection(
  id: SettingsSection | null,
  sections: SettingsSectionDef[] = SETTINGS_SECTIONS,
): SettingsSectionDef {
  return sections.find((s) => s.id === id) ?? sections[0];
}
