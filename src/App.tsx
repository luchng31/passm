import { useEffect, useState } from 'react';
import { getSessionStatus } from './lib/api';
import type { SessionStatus } from './lib/api';
import { SessionContext } from './lib/session';
import type { SessionContextValue } from './lib/session';
import { Header } from './components/Header';
import { Unlock } from './screens/Unlock';
import { VaultList } from './screens/VaultList';

export function App() {
  const [status, setStatus] = useState<SessionStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshTick, setRefreshTick] = useState(0);

  useEffect(() => {
    getSessionStatus()
      .then(setStatus)
      .catch((err) => {
        setError(String(err));
        setStatus({ unlocked: false, device_id: '' });
      });
  }, []);

  // Poll the backend session state so tray "Lock" and the auto-lock timer
  // (both run in Rust, outside the webview) transition the UI to the Unlock
  // screen the same way the manual lock button does. Cheap local IPC call.
  //
  // IMPORTANT: only detect the lock transition (unlocked -> locked). We must
  // NOT set unlocked=true from this poll, otherwise VaultList mounts the
  // instant `unlock()` flips the backend flag — which is BEFORE the
  // post-unlock `syncNow()` reload finishes — so it briefly renders the
  // stale vault and then flips to the synced one (the "old -> new" flash).
  // VaultList is mounted exclusively by the explicit setUnlocked(true) in
  // Unlock.tsx, which fires only after the reload completes.
  useEffect(() => {
    const timer = window.setInterval(() => {
      getSessionStatus()
        .then((s) => {
          setStatus((prev) => {
            if (prev === null) return prev;
            // external/tray/auto lock: true -> false
            if (prev.unlocked && !s.unlocked) {
              return { unlocked: false, device_id: s.device_id };
            }
            return prev;
          });
        })
        .catch(() => undefined);
    }, 2000);
    return () => window.clearInterval(timer);
  }, []);

  if (status === null) {
    return <div className="app-loading">加载中…</div>;
  }

  const contextValue: SessionContextValue = {
    unlocked: status.unlocked,
    deviceId: status.device_id,
    setUnlocked: (unlocked) => setStatus({ unlocked, device_id: status.device_id }),
    refreshTick,
    bumpRefresh: () => setRefreshTick((t) => t + 1),
  };

  return (
    <SessionContext.Provider value={contextValue}>
      <div className="app">
        <Header />
        {error !== null && <div className="error banner">{error}</div>}
        {status.unlocked ? (
          <VaultList />
        ) : (
          <main className="app-main">
            <Unlock />
          </main>
        )}
      </div>
    </SessionContext.Provider>
  );
}

export default App;