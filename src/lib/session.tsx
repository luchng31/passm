import { createContext, useContext } from 'react';

export interface SessionContextValue {
  unlocked: boolean;
  deviceId: string;
  setUnlocked: (unlocked: boolean) => void;
  /** Bumped after a successful sync so mounted screens reload their data. */
  refreshTick: number;
  bumpRefresh: () => void;
}

export const SessionContext = createContext<SessionContextValue>({
  unlocked: false,
  deviceId: '',
  setUnlocked: () => undefined,
  refreshTick: 0,
  bumpRefresh: () => undefined,
});

export function useSession(): SessionContextValue {
  return useContext(SessionContext);
}