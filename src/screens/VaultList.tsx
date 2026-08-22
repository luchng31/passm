import { useCallback, useEffect, useMemo, useState } from 'react';
import { deleteEntry, list } from '../lib/api';
import type { Entry } from '../lib/api';
import { filterEntries } from '../lib/search';
import { ItemEditor } from './ItemEditor';
import { EntryDetail } from './EntryDetail';
import { useSession } from '../lib/session';

function SearchIcon() {
  return (
    <svg className="search-icon" width="15" height="15" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <circle cx="11" cy="11" r="7" stroke="currentColor" strokeWidth="2" />
      <path d="M20 20l-3.2-3.2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

function EmptyMark() {
  return (
    <span className="empty-mark" aria-hidden="true">
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none">
        <rect x="4" y="10" width="16" height="10" rx="2" stroke="currentColor" strokeWidth="1.8" />
        <path d="M8 10V7.5a4 4 0 0 1 8 0V10" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
        <circle cx="12" cy="15" r="1.4" fill="currentColor" />
      </svg>
    </span>
  );
}

/**
 * Two-pane master-detail workspace: compact entry list on the left, read-only
 * detail / editor on the right. Rows select; the right pane acts.
 */
export function VaultList() {
  const { refreshTick } = useSession();
  const [entries, setEntries] = useState<Entry[]>([]);
  const [query, setQuery] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // undefined = detail view, null = create new, Entry = edit that entry
  const [editing, setEditing] = useState<Entry | null | undefined>(undefined);

  const load = useCallback(async () => {
    try {
      setEntries(await list());
      setError(null);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load, refreshTick]);

  const filtered = useMemo(() => filterEntries(entries, query), [entries, query]);
  const selected = useMemo(
    () => entries.find((e) => e.id === selectedId) ?? null,
    [entries, selectedId],
  );

  // Single owner of the delete side effect: both EntryDetail and ItemEditor
  // only notify; this is the one place that talks to the backend and updates
  // list state, so a failed delete surfaces here instead of silently keeping
  // a stale row.
  const handleDelete = async (id: string) => {
    try {
      await deleteEntry(id);
      setEntries((prev) => prev.filter((e) => e.id !== id));
      setSelectedId((prev) => (prev === id ? null : prev));
      setEditing(undefined);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleSaved = useCallback((saved: Entry) => {
    setEntries((prev) => {
      const exists = prev.some((e) => e.id === saved.id);
      return exists ? prev.map((e) => (e.id === saved.id ? saved : e)) : [saved, ...prev];
    });
    setSelectedId(saved.id);
    setEditing(undefined);
  }, []);

  const isEmptyVault = !loading && entries.length === 0;

  if (isEmptyVault) {
    if (editing === undefined) {
      return (
        <div className="workspace workspace--empty">
          <div className="empty-state">
            <EmptyMark />
            <p className="empty-title">保险库为空，点击"新建"添加第一个条目</p>
            <p className="empty-hint">密码会加密保存在本地，并随同步仓库备份。</p>
            <button className="btn btn-primary" onClick={() => setEditing(null)}>
              新建
            </button>
          </div>
        </div>
      );
    }
    // Creating the very first entry: there is no list yet, so the editor
    // takes the whole workspace. editing can only be null (create) here.
    return (
      <div className="workspace workspace--empty">
        <ItemEditor
          entry={editing}
          onSaved={handleSaved}
          onCancel={() => setEditing(undefined)}
          onDelete={handleDelete}
        />
      </div>
    );
  }

  return (
    <div className="workspace">
      <nav className="pane-list" aria-label="密码条目列表">
        <div className="list-toolbar">
          <div className="search">
            <SearchIcon />
            <input
              className="input search-box"
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="搜索…"
            />
          </div>
          <button className="btn btn-primary" onClick={() => setEditing(null)}>
            新建
          </button>
        </div>

        {loading ? (
          <ul className="skeleton-list" aria-hidden="true">
            {[0, 1, 2, 3].map((i) => (
              <li className="skeleton-row" key={i}>
                <div className="skeleton-avatar" />
                <div className="skeleton-lines">
                  <div className="skeleton-line skeleton-line--title" />
                  <div className="skeleton-line skeleton-line--sub" />
                </div>
              </li>
            ))}
          </ul>
        ) : (
          <>
            {error !== null && (
              <div className="error" style={{ margin: 'var(--space-3)' }}>
                {error}
              </div>
            )}
            {filtered.length === 0 ? (
              <p className="muted">没有匹配的条目，换个关键词试试。</p>
            ) : (
              <ul className="entry-list">
                {filtered.map((entry) => (
                  <li key={entry.id}>
                    <button
                      type="button"
                      className={`entry-row${entry.id === selectedId ? ' is-selected' : ''}`}
                      onClick={() => {
                        setSelectedId(entry.id);
                        setEditing(undefined);
                      }}
                    >
                      <span className="avatar" aria-hidden="true">
                        {entry.title.trim().charAt(0).toUpperCase() || '•'}
                      </span>
                      <span className="entry-info">
                        <span className="entry-title">{entry.title}</span>
                        <span className="entry-username">{entry.username}</span>
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </>
        )}
      </nav>

      <section className="pane-detail" aria-label="条目详情">
        {editing !== undefined ? (
          <ItemEditor
            entry={editing}
            onSaved={handleSaved}
            onCancel={() => setEditing(undefined)}
            onDelete={handleDelete}
          />
        ) : selected !== null ? (
          <EntryDetail
            key={selected.id}
            entry={selected}
            onEdit={(entry) => setEditing(entry)}
            onDelete={handleDelete}
          />
        ) : (
          <div className="detail-placeholder">
            <EmptyMark />
            <span>从左侧选择一个条目查看详情</span>
          </div>
        )}
      </section>
    </div>
  );
}
