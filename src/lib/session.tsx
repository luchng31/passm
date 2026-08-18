import { createContext, useContext } from 'react';

export interface SessionContextValue {
  unlocked: boolean;
  deviceId: string;
  setUnlocked: (unlocked: boolean) => void;
}

export const SessionContext = createContext<SessionContextValue>({
  unlocked: false,
  deviceId: '',
  setUnlocked: () => undefined,
});

export function useSession(): SessionContextValue {
  return useContext(SessionContext);
}