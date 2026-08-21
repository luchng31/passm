import { useCallback, useEffect, useMemo, useState } from 'react';
import { copy, deleteEntry, list } from '../lib/api';
import type { Entry } from '../lib/api';
import type { CopyField } from '../lib/copy';
import { useCopyTimer } from '../lib/copy';
import { filterEntries } from '../lib/search';
import { ConfirmButton } from '../components/ConfirmButton';
import { ItemEditor } from './ItemEditor';
import { useSession } from '../lib/session';

const AVATAR_COLORS = [
  '#4f46e5',
  '#0a84ff',
  '#30b0c7',
  '#30d158',
  '#ff9f0a',
  '#ff375f',
  '#bf5af2',
  '#ff6482',
];

function avatarColor(title: string): string {
  let hash = 0;
  for (let i = 0; i < title.length; i++) {
    hash = (hash * 31 + title.charCodeAt(i)) >>> 0;
  }
  return AVATAR_COLORS[hash % AVATAR_COLORS.length];
}

function SearchIcon() {
  return (
    <svg className="search-icon" width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <circle cx="11" cy="11" r="7" stroke="currentColor" strokeWidth="2" />
      <path d="M20 20l-3.2-3.2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export function VaultList() {
  const { refreshTick } = useSession();
  const [entries, setEntries] = useState<Entry[]>([]);
  const [query, setQuery] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // undefined = list view, null = create new, Entry = edit that entry
  const [editing, setEditing] = useState<Entry | null | undefined>(undefined);
  const { copiedField, copiedId, copyWithTimer } = useCopyTimer();

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

  const handleCopy = async (field: CopyField, id: string) => {
    try {
      await copy(field, id);
      copyWithTimer(field, id);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteEntry(id);
      setEntries((prev) => prev.filter((e) => e.id !== id));
    } catch (err) {
      setError(String(err));
    }
  };

  if (editing !== undefined) {
    return (
      <ItemEditor
        entry={editing}
        onSaved={(saved) => {
          setEntries((prev) => {
            const exists = prev.some((e) => e.id === saved.id);
            return exists ? prev.map((e) => (e.id === saved.id ? saved : e)) : [saved, ...prev];
          });
          setEditing(undefined);
        }}
        onCancel={() => setEditing(undefined)}
        onDeleted={(id) => {
          setEntries((prev) => prev.filter((e) => e.id !== id));
          setEditing(undefined);
        }}
      />
    );
  }

  return (
    <div className="vault">
      <div className="vault-toolbar">
        <div className="search">
          <SearchIcon />
          <input
            className="input search-box"
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="搜索标题、用户名或网址…"
          />
        </div>
        <button className="btn btn-primary" onClick={() => setEditing(null)}>
          新建
        </button>
      </div>
      {error !== null && <div className="error">{error}</div>}
      {loading ? (
        <p className="muted">
          <span className="spinner" aria-hidden="true" />
          加载中…
        </p>
      ) : filtered.length === 0 ? (
        <p className="muted">
          {entries.length === 0 ? '保险库为空，点击"新建"添加第一个条目' : '没有匹配的条目'}
        </p>
      ) : (
        <ul className="entry-list">
          {filtered.map((entry, index) => (
            <li key={entry.id} className="entry-card" style={{ animationDelay: `${Math.min(index, 12) * 28}ms` }}>
              <div className="entry-main">
                <div className="avatar" style={{ background: avatarColor(entry.title) }} aria-hidden="true">
                  {entry.title.trim().charAt(0).toUpperCase() || '•'}
                </div>
                <div className="entry-info">
                  <span className="entry-title">{entry.title}</span>
                  <span className="entry-username">{entry.username}</span>
                </div>
              </div>
              <div className="entry-actions">
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={() => void handleCopy('password', entry.id)}
                >
                  {copiedField === 'password' && copiedId === entry.id ? '已复制' : '复制密码'}
                </button>
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={() => void handleCopy('username', entry.id)}
                >
                  {copiedField === 'username' && copiedId === entry.id ? '已复制' : '复制用户名'}
                </button>
                <button className="btn btn-ghost btn-sm" onClick={() => setEditing(entry)}>
                  编辑
                </button>
                <ConfirmButton label="删除" onConfirm={() => void handleDelete(entry.id)} />
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
