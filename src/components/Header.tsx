import { useState } from 'react';
import { lock } from '../lib/api';
import { useSession } from '../lib/session';
import { SyncStatus } from './SyncStatus';

export function Header() {
  const { unlocked, setUnlocked } = useSession();
  const [error, setError] = useState<string | null>(null);

  const handleLock = async () => {
    try {
      await lock();
      setUnlocked(false);
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <header className="app-header">
      <h1 className="app-title">passm</h1>
      <div className="app-header-right">
        {error !== null && <span className="error">{error}</span>}
        {unlocked && <SyncStatus />}
        {unlocked && (
          <button className="btn btn-sm" onClick={() => void handleLock()}>
            锁定
          </button>
        )}
      </div>
    </header>
  );
}