import { useState } from 'react';
import { syncNow } from '../lib/api';
import type { SyncStatus as SyncStatusResult } from '../lib/api';
import { useSession } from '../lib/session';

/**
 * Sync status indicator: manual "同步" button plus the last sync outcome
 * (pushed / pulled / merged / backup) and any error from the backend.
 */
export function SyncStatus() {
  const { bumpRefresh } = useSession();
  const [result, setResult] = useState<SyncStatusResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);

  const handleSync = async () => {
    setSyncing(true);
    setError(null);
    try {
      setResult(await syncNow());
      bumpRefresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setSyncing(false);
    }
  };

  const actions: string[] = [];
  if (result !== null) {
    if (result.pushed) actions.push('已推送');
    if (result.pulled) actions.push('已拉取');
    if (result.merged) actions.push('已合并');
    if (actions.length === 0) actions.push('已是最新');
    if (result.backup_created !== null) actions.push('已备份');
  }

  return (
    <div className="sync-status">
      <button className="btn btn-ghost btn-sm" onClick={() => void handleSync()} disabled={syncing}>
        {syncing ? '同步中…' : '同步'}
      </button>
      {error !== null && (
        <span className="sync-error" title={error}>
          {error}
        </span>
      )}
      {result !== null && (
        <span className="sync-result" title={result.backup_created ?? undefined}>
          上次同步: {actions.join(' · ')}
        </span>
      )}
    </div>
  );
}