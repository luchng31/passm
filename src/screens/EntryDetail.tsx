import { useState } from 'react';
import { copy } from '../lib/api';
import type { CopyField, Entry } from '../lib/api';
import { useCopyTimer } from '../lib/copy';
import { ConfirmButton } from '../components/ConfirmButton';

interface EntryDetailProps {
  entry: Entry;
  onEdit: (entry: Entry) => void;
  /** Notify the workspace to delete this entry; the workspace owns the
   * backend call and list-state update (single source of truth). */
  onDelete: (id: string) => void;
}

function CopyIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <rect x="9" y="9" width="11" height="11" rx="2.5" stroke="currentColor" strokeWidth="1.8" />
      <path d="M5 15V6a2 2 0 0 1 2-2h9" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M5 12.5l4.5 4.5L19 7" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function EyeIcon({ off }: { off?: boolean }) {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path
        d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinejoin="round"
      />
      <circle cx="12" cy="12" r="2.75" stroke="currentColor" strokeWidth="1.8" />
      {!off && <circle cx="12" cy="12" r="1.1" fill="currentColor" />}
      {off && (
        <path d="M4 20L20 4" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
      )}
    </svg>
  );
}

/** Unix seconds (or ms) -> localized short date; falsy/invalid renders a dash. */
function formatDate(unix: number): string {
  if (!unix) return '—';
  const ms = unix < 1e12 ? unix * 1000 : unix;
  const d = new Date(ms);
  return Number.isNaN(d.getTime()) ? '—' : d.toLocaleString();
}

export function EntryDetail({ entry, onEdit, onDelete }: EntryDetailProps) {
  const [showPassword, setShowPassword] = useState(false);
  const { copiedField, copiedId, copyWithTimer } = useCopyTimer();

  const handleCopy = async (field: CopyField) => {
    try {
      await copy(field, entry.id);
      copyWithTimer(field, entry.id);
    } catch {
      // copy failures are surfaced by the backend toast path in other screens;
      // here the button simply stays unchanged.
    }
  };

  const isCopied = (field: CopyField) => copiedField === field && copiedId === entry.id;

  const copyBtn = (field: CopyField) => (
    <button
      type="button"
      className={`icon-btn${isCopied(field) ? ' is-copied' : ''}`}
      onClick={() => void handleCopy(field)}
      title={isCopied(field) ? '已复制' : '复制'}
      aria-label={isCopied(field) ? '已复制' : '复制'}
    >
      {isCopied(field) ? <CheckIcon /> : <CopyIcon />}
    </button>
  );

  return (
    <div className="detail">
      <header className="detail-head">
        <div className="avatar" aria-hidden="true">
          {entry.title.trim().charAt(0).toUpperCase() || '•'}
        </div>
        <div style={{ minWidth: 0 }}>
          <h2 className="detail-title">{entry.title}</h2>
          <div className="detail-subtitle">{entry.username}</div>
        </div>
        <div className="detail-actions">
          <button type="button" className="btn btn-sm" onClick={() => onEdit(entry)}>
            编辑
          </button>
          <ConfirmButton
            label="删除"
            className="btn btn-danger btn-sm"
            onConfirm={() => onDelete(entry.id)}
          />
        </div>
      </header>

      {entry.username !== '' && (
        <section className="field-block">
          <div className="field-label">用户名</div>
          <div className="field-value">
            <span>{entry.username}</span>
            {copyBtn('username')}
          </div>
        </section>
      )}

      <section className="field-block">
        <div className="field-label">密码</div>
        <div className="field-value">
          {showPassword ? (
            <span className="field-value--mono">{entry.password}</span>
          ) : (
            <span className="field-value--mono field-value--masked">
              {'•'.repeat(Math.min(entry.password.length || 8, 14))}
            </span>
          )}
          <button
            type="button"
            className="icon-btn"
            onClick={() => setShowPassword((v) => !v)}
            title={showPassword ? '隐藏密码' : '显示密码'}
            aria-label={showPassword ? '隐藏密码' : '显示密码'}
            aria-pressed={showPassword}
          >
            <EyeIcon off={showPassword} />
          </button>
          {copyBtn('password')}
        </div>
      </section>

      <section className="field-block">
        <div className="field-label">网址</div>
        <div className="field-value">
          {entry.url !== '' ? (
            <>
              <a href={entry.url} target="_blank" rel="noopener noreferrer">
                {entry.url}
              </a>
              {copyBtn('url')}
            </>
          ) : (
            <span className="field-empty">未设置</span>
          )}
        </div>
      </section>

      <section className="field-block">
        <div className="field-label">备注</div>
        <div className="field-value">
          {entry.notes !== '' ? (
            <span style={{ whiteSpace: 'pre-wrap' }}>{entry.notes}</span>
          ) : (
            <span className="field-empty">无备注</span>
          )}
        </div>
      </section>

      <section className="field-block">
        <div className="field-label">更新时间</div>
        <div className="field-value">
          <span className="field-empty">{formatDate(entry.updated_at)}</span>
        </div>
      </section>
    </div>
  );
}
