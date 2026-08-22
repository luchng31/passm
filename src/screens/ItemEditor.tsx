import { useState } from 'react';
import { create, generatePassword, update } from '../lib/api';
import type { Entry, EntryInput } from '../lib/api';
import { ConfirmButton } from '../components/ConfirmButton';

interface ItemEditorProps {
  /** null = create new entry, otherwise edit this entry. */
  entry: Entry | null;
  onSaved: (entry: Entry) => void;
  onCancel: () => void;
  /** Notify the workspace to delete this entry; the workspace owns the
   * backend call and list-state update (single source of truth). */
  onDelete: (id: string) => void;
}

function passwordStrength(pw: string): number {
  if (pw.length === 0) return 0;
  let score = 0;
  if (pw.length >= 8) score++;
  if (pw.length >= 12) score++;
  if (/[a-z]/.test(pw) && /[A-Z]/.test(pw)) score++;
  if (/\d/.test(pw)) score++;
  if (/[^A-Za-z0-9]/.test(pw)) score++;
  return Math.min(score, 4);
}

const STRENGTH_TEXT = ['过短', '弱', '一般', '较强', '强'] as const;

export function ItemEditor({ entry, onSaved, onCancel, onDelete }: ItemEditorProps) {
  const [title, setTitle] = useState(entry?.title ?? '');
  const [username, setUsername] = useState(entry?.username ?? '');
  const [password, setPassword] = useState(entry?.password ?? '');
  const [url, setUrl] = useState(entry?.url ?? '');
  const [notes, setNotes] = useState(entry?.notes ?? '');
  const [showPassword, setShowPassword] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const strength = passwordStrength(password);
  const strengthLevel = strength <= 1 ? 'weak' : strength === 2 ? 'medium' : 'strong';

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      const input: EntryInput = { title, username, password, url, notes };
      const saved = entry === null ? await create(input) : await update(entry.id, input);
      onSaved(saved);
    } catch (err) {
      setError(String(err));
      setSaving(false);
    }
  };

  const handleGenerate = async () => {
    try {
      setPassword(await generatePassword(16));
      setShowPassword(true);
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <div className="editor">
      <h2 className="editor-title">{entry === null ? '新建条目' : '编辑条目'}</h2>
      {error !== null && <div className="error" style={{ marginBottom: 'var(--space-4)' }}>{error}</div>}
      <form
        className="editor-form"
        onSubmit={(e) => {
          e.preventDefault();
          void handleSave();
        }}
      >
        <label className="field">
          <span>标题</span>
          <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} autoFocus />
        </label>
        <label className="field">
          <span>用户名</span>
          <input className="input" value={username} onChange={(e) => setUsername(e.target.value)} />
        </label>
        <label className="field">
          <span>密码</span>
          <div className="field-row">
            <input
              className="input input-mono"
              type={showPassword ? 'text' : 'password'}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
            <button type="button" className="btn" onClick={() => setShowPassword((v) => !v)}>
              {showPassword ? '隐藏' : '显示'}
            </button>
            <button type="button" className="btn" onClick={() => void handleGenerate()}>
              生成
            </button>
          </div>
          {password.length > 0 && (
            <div aria-hidden="true">
              <div className="strength">
                {[0, 1, 2, 3].map((i) => (
                  <span
                    key={i}
                    className={`strength-bar${i < strength ? ` is-${strengthLevel}` : ''}`}
                  />
                ))}
              </div>
              <span className="strength-text">强度：{STRENGTH_TEXT[strength]}</span>
            </div>
          )}
        </label>
        <label className="field">
          <span>网址</span>
          <input className="input" value={url} onChange={(e) => setUrl(e.target.value)} />
        </label>
        <label className="field">
          <span>备注</span>
          <textarea className="input" rows={4} value={notes} onChange={(e) => setNotes(e.target.value)} />
        </label>
        <div className="editor-actions">
          <button type="submit" className="btn btn-primary" disabled={saving}>
            {saving && <span className="spinner" aria-hidden="true" />}
            {saving ? '保存中…' : '保存'}
          </button>
          <button type="button" className="btn" onClick={onCancel} disabled={saving}>
            取消
          </button>
          <span className="spacer" />
          {entry !== null && (
            <ConfirmButton
              label="删除"
              className="btn btn-danger btn-sm"
              onConfirm={() => onDelete(entry.id)}
            />
          )}
        </div>
      </form>
    </div>
  );
}
