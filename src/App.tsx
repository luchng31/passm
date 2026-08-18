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

  useEffect(() => {
    getSessionStatus()
      .then(setStatus)
      .catch((err) => {
        setError(String(err));
        setStatus({ unlocked: false, device_id: '' });
      });
  }, []);

  if (status === null) {
    return <div className="app-loading">加载中…</div>;
  }

  const contextValue: SessionContextValue = {
    unlocked: status.unlocked,
    deviceId: status.device_id,
    setUnlocked: (unlocked) => setStatus({ unlocked, device_id: status.device_id }),
  };

  return (
    <SessionContext.Provider value={contextValue}>
      <div className="app">
        <Header />
        {error !== null && <div className="error banner">{error}</div>}
        <main className="app-main">{status.unlocked ? <VaultList /> : <Unlock />}</main>
      </div>
    </SessionContext.Provider>
  );
}

export default App;