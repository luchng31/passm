import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { TouchEvent as ReactTouchEvent } from 'react';
import { copy, deleteEntry, list } from '../lib/api';
import type { Entry } from '../lib/api';
import type { CopyField } from '../lib/copy';
import { useCopyTimer } from '../lib/copy';
import { filterEntries } from '../lib/search';
import { ItemEditor } from './ItemEditor';
import { useSession } from '../lib/session';

/**
 * Calm, premium avatar differentiation: a single accent hue (see --avatar-bg /
 * --avatar-fg tokens) rotated slightly per entry so entries stay distinct
 * without breaking the color-consistency lock. No rainbow primaries.
 */
function avatarHue(title: string): number {
  let hash = 0;
  for (let i = 0; i < title.length; i++) {
    hash = (hash * 31 + title.charCodeAt(i)) >>> 0;
  }
  return ((hash % 7) - 3) * 16; // -48deg .. +48deg around the accent hue
}

function SearchIcon() {
  return (
    <svg className="search-icon" width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <circle cx="11" cy="11" r="7" stroke="currentColor" strokeWidth="2" />
      <path d="M20 20l-3.2-3.2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

function CopyIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <rect x="9" y="9" width="11" height="11" rx="2.5" stroke="currentColor" strokeWidth="2" />
      <path d="M5 15V6a2 2 0 0 1 2-2h9" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M5 12.5l4.5 4.5L19 7" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function EmptyIllustration() {
  return (
    <svg className="empty-illustration" width="36" height="36" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <rect x="4" y="10" width="16" height="10" rx="3" stroke="currentColor" strokeWidth="2" />
      <path d="M8 10V7.5a4 4 0 0 1 8 0V10" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
      <circle cx="12" cy="15" r="1.6" fill="currentColor" />
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

  // 编辑 / 删除 are reached via right-click (desktop) or long-press (touch)
  // on the card, not via inline buttons, to keep the row to the two copy
  // actions only. Delete is a two-step confirm inside the menu.
  const [menu, setMenu] = useState<{ id: string; x: number; y: number; confirming: boolean } | null>(
    null,
  );
  const longPressTimer = useRef<number | null>(null);
  const longPressFired = useRef(false);

  const openMenu = (entry: Entry, x: number, y: number) => {
    setMenu({ id: entry.id, x, y, confirming: false });
  };
  const closeMenu = () => setMenu(null);

  const startLongPress = (e: ReactTouchEvent<HTMLLIElement>, entry: Entry) => {
    longPressFired.current = false;
    const touch = e.touches[0];
    const rect = e.currentTarget.getBoundingClientRect();
    const x = touch.clientX - rect.left;
    const y = touch.clientY - rect.top;
    longPressTimer.current = window.setTimeout(() => {
      longPressFired.current = true;
      openMenu(entry, x, y);
    }, 500);
  };
  const cancelLongPress = () => {
    if (longPressTimer.current !== null) {
      window.clearTimeout(longPressTimer.current);
      longPressTimer.current = null;
    }
  };
  const endLongPress = (e: ReactTouchEvent<HTMLLIElement>) => {
    if (longPressFired.current) {
      // suppress the click that follows a long-press
      e.preventDefault();
      longPressFired.current = false;
    }
    cancelLongPress();
  };

  useEffect(() => {
    if (menu === null) return;
    const onDocClick = (e: MouseEvent) => {
      if ((e.target as HTMLElement).closest('.entry-menu')) return;
      closeMenu();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeMenu();
    };
    document.addEventListener('click', onDocClick);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('click', onDocClick);
      document.removeEventListener('keydown', onKey);
    };
  }, [menu]);

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
        <ul className="skeleton-list" aria-hidden="true">
          {[0, 1, 2].map((i) => (
            <li className="skeleton-row" key={i}>
              <div className="skeleton-head">
                <div className="skeleton-avatar" />
                <div className="skeleton-lines">
                  <div className="skeleton-line skeleton-line--title" />
                  <div className="skeleton-line skeleton-line--sub" />
                </div>
              </div>
              <div className="skeleton-btn" />
            </li>
          ))}
        </ul>
      ) : filtered.length === 0 ? (
        entries.length === 0 ? (
          <div className="empty-state">
            <EmptyIllustration />
            <p className="empty-title">保险库为空，点击"新建"添加第一个条目</p>
            <p className="empty-hint">密码会加密保存在本地，并随同步仓库备份。</p>
            <button className="btn btn-primary" onClick={() => setEditing(null)}>
              新建
            </button>
          </div>
        ) : (
          <div className="empty-state">
            <EmptyIllustration />
            <p className="empty-title">没有匹配的条目</p>
            <p className="empty-hint">尝试更换关键词，或清空搜索框查看全部。</p>
          </div>
        )
      ) : (
        <ul className="entry-list">
          {filtered.map((entry, index) => (
            <li
              key={entry.id}
              className="entry-card"
              style={{ animationDelay: `${Math.min(index, 12) * 28}ms` }}
              onContextMenu={(e) => {
                e.preventDefault();
                const rect = e.currentTarget.getBoundingClientRect();
                openMenu(entry, e.clientX - rect.left, e.clientY - rect.top);
              }}
              onTouchStart={(e) => startLongPress(e, entry)}
              onTouchEnd={endLongPress}
              onTouchMove={cancelLongPress}
            >
              <div className="entry-main">
                <div
                  className="avatar"
                  style={{ filter: `hue-rotate(${avatarHue(entry.title)}deg)` }}
                  aria-hidden="true"
                >
                  {entry.title.trim().charAt(0).toUpperCase() || '•'}
                </div>
                <div className="entry-info">
                  <span className="entry-title">{entry.title}</span>
                  <span className="entry-username">{entry.username}</span>
                </div>
              </div>
              <button
                className={`btn entry-copy-primary${
                  copiedField === 'password' && copiedId === entry.id ? ' is-copied' : ''
                }`}
                onClick={() => void handleCopy('password', entry.id)}
              >
                {copiedField === 'password' && copiedId === entry.id ? <CheckIcon /> : <CopyIcon />}
                {copiedField === 'password' && copiedId === entry.id ? '已复制' : '复制密码'}
              </button>
              <div className="entry-secondary">
                <button className="btn btn-ghost" onClick={() => void handleCopy('username', entry.id)}>
                  {copiedField === 'username' && copiedId === entry.id ? '已复制' : '复制用户名'}
                </button>
              </div>
              {menu?.id === entry.id && (
                <div className="entry-menu" style={{ top: menu.y, left: menu.x }} role="menu">
                  {!menu.confirming ? (
                    <>
                      <button
                        type="button"
                        className="entry-menu-item"
                        role="menuitem"
                        onClick={() => {
                          setEditing(entry);
                          closeMenu();
                        }}
                      >
                        编辑
                      </button>
                      <button
                        type="button"
                        className="entry-menu-item entry-menu-item--danger"
                        role="menuitem"
                        onClick={() => setMenu((m) => (m ? { ...m, confirming: true } : m))}
                      >
                        删除
                      </button>
                    </>
                  ) : (
                    <>
                      <span className="entry-menu-confirm">确认删除?</span>
                      <button
                        type="button"
                        className="entry-menu-item entry-menu-item--danger"
                        role="menuitem"
                        onClick={() => {
                          void handleDelete(entry.id);
                          closeMenu();
                        }}
                      >
                        确认
                      </button>
                      <button
                        type="button"
                        className="entry-menu-cancel"
                        role="menuitem"
                        onClick={() => setMenu((m) => (m ? { ...m, confirming: false } : m))}
                      >
                        取消
                      </button>
                    </>
                  )}
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
