import { create } from 'zustand';
import {
  type ComplianceProfile,
  getComplianceProfile,
  listAuditEvents,
  type SecurityAuditEvent,
  setComplianceProfile,
} from '@/lib/tauri-api';

interface AuditStoreState {
  events: SecurityAuditEvent[];
  profile: ComplianceProfile | null;
  loading: boolean;
  error: string | null;

  loadEvents: () => Promise<void>;
  loadProfile: () => Promise<void>;
  saveProfile: (profile: ComplianceProfile) => Promise<void>;
}

export const useAuditStore = create<AuditStoreState>((set) => ({
  events: [],
  profile: null,
  loading: false,
  error: null,

  loadEvents: async () => {
    set({ loading: true, error: null });
    try {
      const events = await listAuditEvents();
      set({ events, loading: false });
    } catch (e) {
      set({ error: e instanceof Error ? e.message : String(e), loading: false });
    }
  },

  loadProfile: async () => {
    set({ error: null });
    try {
      const profile = await getComplianceProfile();
      set({ profile });
    } catch (e) {
      set({ error: e instanceof Error ? e.message : String(e) });
    }
  },

  saveProfile: async (profile) => {
    await setComplianceProfile(profile);
    set({ profile });
  },
}));
