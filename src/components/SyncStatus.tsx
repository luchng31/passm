import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { syncNow } from '../lib/api';
import type { SyncStatus as SyncStatusResult } from '../lib/api';
import { useSession } from '../lib/session';

/**
 * Sync status indicator: manual "同步" button plus the last sync outcome
 * (pushed / pulled / merged / backup) and any error from the backend.
 *
 * The post-sync result/error is shown as a centered toast inside the header
 * bar (portaled into `.app-header`) and auto-dismisses 30s after it appears;
 * a new sync re-arms the timer. The in-progress "同步中…" stays on the button.
 */
export function SyncStatus() {
  const { bumpRefresh } = useSession();
  const [result, setResult] = useState<SyncStatusResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);

  // Auto-dismiss the result/error toast 30s after it appears; re-armed on
  // every new result/error so a fresh sync resets the countdown.
  useEffect(() => {
    if (result === null && error === null) return;
    const timer = window.setTimeout(() => {
      setResult(null);
      setError(null);
    }, 30000);
    return () => window.clearTimeout(timer);
  }, [result, error]);

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

  const headerEl = typeof document !== 'undefined' ? document.querySelector('.app-header') : null;

  return (
    <>
      <button className="btn btn-sm" onClick={() => void handleSync()} disabled={syncing}>
        {syncing && <span className="spinner" aria-hidden="true" />}
        {syncing ? '同步中…' : '同步'}
      </button>
      {headerEl !== null && (result !== null || error !== null) &&
        createPortal(
          <div className="sync-toast" role="status" aria-live="polite">
            {error !== null ? (
              <span className="sync-error" title={error}>
                {error}
              </span>
            ) : (
              <span className="sync-pill" title={result?.backup_created ?? undefined}>
                上次同步: {actions.join(' · ')}
              </span>
            )}
          </div>,
          headerEl,
        )}
    </>
  );
}
