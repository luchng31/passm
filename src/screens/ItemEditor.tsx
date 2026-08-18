import { useState } from 'react';
import { copy, create, deleteEntry, generatePassword, update } from '../lib/api';
import type { Entry, EntryInput } from '../lib/api';
import type { CopyField } from '../lib/copy';
import { useCopyTimer } from '../lib/copy';
import { ConfirmButton } from '../components/ConfirmButton';

interface ItemEditorProps {
  /** null = create new entry, otherwise edit this entry. */
  entry: Entry | null;
  onSaved: (entry: Entry) => void;
  onCancel: () => void;
  onDeleted: (id: string) => void;
}

export function ItemEditor({ entry, onSaved, onCancel, onDeleted }: ItemEditorProps) {
  const [title, setTitle] = useState(entry?.title ?? '');
  const [username, setUsername] = useState(entry?.username ?? '');
  const [password, setPassword] = useState(entry?.password ?? '');
  const [url, setUrl] = useState(entry?.url ?? '');
  const [notes, setNotes] = useState(entry?.notes ?? '');
  const [showPassword, setShowPassword] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { copiedField, copiedId, copyWithTimer } = useCopyTimer();

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
    } catch (err) {
      setError(String(err));
    }
  };

  const handleCopy = async (field: CopyField) => {
    if (entry === null) return;
    try {
      await copy(field, entry.id);
      copyWithTimer(field, entry.id);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteEntry(id);
      onDeleted(id);
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <div className="editor">
      <h2>{entry === null ? '新建条目' : '编辑条目'}</h2>
      {error !== null && <div className="error">{error}</div>}
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
              className="input"
              type={showPassword ? 'text' : 'password'}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
            <button type="button" className="btn btn-ghost" onClick={() => setShowPassword((v) => !v)}>
              {showPassword ? '隐藏' : '显示'}
            </button>
            <button type="button" className="btn btn-ghost" onClick={() => void handleGenerate()}>
              生成密码
            </button>
          </div>
        </label>
        <label className="field">
          <span>网址</span>
          <input className="input" value={url} onChange={(e) => setUrl(e.target.value)} />
        </label>
        <label className="field">
          <span>备注</span>
          <textarea className="input" rows={4} value={notes} onChange={(e) => setNotes(e.target.value)} />
        </label>
        {entry !== null && (
          <div className="field">
            <span>复制</span>
            <div className="field-row">
              <button type="button" className="btn btn-ghost" onClick={() => void handleCopy('password')}>
                {copiedField === 'password' && copiedId === entry.id ? '已复制' : '复制密码'}
              </button>
              <button type="button" className="btn btn-ghost" onClick={() => void handleCopy('username')}>
                {copiedField === 'username' && copiedId === entry.id ? '已复制' : '复制用户名'}
              </button>
              <button type="button" className="btn btn-ghost" onClick={() => void handleCopy('url')}>
                {copiedField === 'url' && copiedId === entry.id ? '已复制' : '复制网址'}
              </button>
            </div>
          </div>
        )}
        <div className="editor-actions">
          <button type="submit" className="btn btn-primary" disabled={saving}>
            {saving ? '保存中…' : '保存'}
          </button>
          <button type="button" className="btn btn-ghost" onClick={onCancel} disabled={saving}>
            取消
          </button>
          {entry !== null && (
            <ConfirmButton label="删除" onConfirm={() => void handleDelete(entry.id)} />
          )}
        </div>
      </form>
    </div>
  );
}