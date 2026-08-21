import { useEffect, useState } from 'react';
import { createVault, getSyncConfig, hasVault, setSyncConfig, syncNow, unlock } from '../lib/api';
import { useSession } from '../lib/session';

type Stage = 'loading' | 'config' | 'create' | 'unlock';

function LockMark() {
  return (
    <svg width="26" height="26" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <rect x="4.5" y="10.5" width="15" height="10" rx="3" fill="currentColor" opacity="0.95" />
      <path
        d="M7.5 10.5V8a4.5 4.5 0 0 1 9 0v2.5"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        fill="none"
      />
      <circle cx="12" cy="15.5" r="1.6" fill="#fff" />
    </svg>
  );
}

export function Unlock() {
  const { setUnlocked, bumpRefresh } = useSession();
  const [stage, setStage] = useState<Stage>('loading');
  // config form
  const [remoteUrl, setRemoteUrl] = useState('');
  const [pat, setPat] = useState('');
  // create/unlock form
  const [password, setPassword] = useState('');
  const [passwordConfirm, setPasswordConfirm] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const config = await getSyncConfig();
        if (config === null) {
          if (!cancelled) setStage('config');
          return;
        }
        const vaultExists = await hasVault();
        if (!cancelled) setStage(vaultExists ? 'unlock' : 'create');
      } catch (err) {
        if (!cancelled) setError(String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const handleConfig = async (event: React.FormEvent) => {
    event.preventDefault();
    setLoading(true);
    setError(null);
    try {
      await setSyncConfig(remoteUrl.trim(), pat);
      const vaultExists = await hasVault();
      setStage(vaultExists ? 'unlock' : 'create');
      setPat('');
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleCreate = async (event: React.FormEvent) => {
    event.preventDefault();
    if (password.length === 0) {
      setError('请输入主密码');
      return;
    }
    if (password !== passwordConfirm) {
      setError('两次输入的密码不一致');
      return;
    }
    setLoading(true);
    setError(null);
    try {
      await createVault(password);
      setPassword('');
      setPasswordConfirm('');
      setUnlocked(true);
    } catch (err) {
      setError(String(err));
      setLoading(false);
    }
  };

  const handleUnlock = async (event: React.FormEvent) => {
    event.preventDefault();
    setLoading(true);
    setError(null);
    try {
      await unlock(password);
      // 后端 unlock 内部已尽力同步，但部分旧构建(certfix9)解锁后内存 vault
      // 未刷新，导致列表仍显示同步前的数据。这里再走一次与手动“同步”相同的
      // 可靠 reload 路径(空闲时幂等)，并 bumpRefresh 让 VaultList 重新拉取，
      // 确保解锁时从远端拉取到的【新增/变更】能立即显示。
      try {
        await syncNow();
      } catch {
        // 离线或未配置同步时忽略：此时无远端变更，本地即为最新
      }
      bumpRefresh();
      setPassword('');
      setUnlocked(true);
    } catch (err) {
      setError(String(err));
      setLoading(false);
    }
  };

  if (stage === 'loading') {
    return (
      <div className="unlock-screen">
        <div className="app-loading">加载中…</div>
      </div>
    );
  }

  if (stage === 'config') {
    return (
      <div className="unlock-screen">
        <form className="unlock-card" onSubmit={(e) => void handleConfig(e)}>
          <div className="unlock-brand">
            <span className="unlock-mark">
              <LockMark />
            </span>
            <h2 className="unlock-title">首次使用</h2>
            <p className="unlock-hint">配置同步仓库以开始使用</p>
          </div>
          <input
            className="input"
            type="text"
            value={remoteUrl}
            onChange={(e) => setRemoteUrl(e.target.value)}
            placeholder="GitHub 仓库地址"
            autoFocus
            disabled={loading}
          />
          <input
            className="input"
            type="password"
            value={pat}
            onChange={(e) => setPat(e.target.value)}
            placeholder="个人访问令牌 (PAT)"
            disabled={loading}
          />
          {error !== null && <div className="error">{error}</div>}
          <div className="unlock-actions">
            <button
              className="btn btn-primary btn-block"
              type="submit"
              disabled={loading || remoteUrl.trim().length === 0 || pat.length === 0}
            >
              {loading && <span className="spinner" aria-hidden="true" />}
              {loading ? '保存中…' : '保存配置'}
            </button>
          </div>
          <p className="unlock-hint">PAT 仅保存在系统密钥库中，不会写入仓库</p>
        </form>
      </div>
    );
  }

  if (stage === 'create') {
    return (
      <div className="unlock-screen">
        <form className="unlock-card" onSubmit={(e) => void handleCreate(e)}>
          <div className="unlock-brand">
            <span className="unlock-mark">
              <LockMark />
            </span>
            <h2 className="unlock-title">创建保险库</h2>
            <p className="unlock-hint">设置主密码（请务必牢记，忘记无法找回）</p>
          </div>
          <input
            className="input"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="主密码"
            autoFocus
            disabled={loading}
          />
          <input
            className="input"
            type="password"
            value={passwordConfirm}
            onChange={(e) => setPasswordConfirm(e.target.value)}
            placeholder="确认主密码"
            disabled={loading}
          />
          {error !== null && <div className="error">{error}</div>}
          <div className="unlock-actions">
            <button
              className="btn btn-primary btn-block"
              type="submit"
              disabled={loading || password.length === 0 || passwordConfirm.length === 0}
            >
              {loading && <span className="spinner" aria-hidden="true" />}
              {loading ? '创建中…' : '创建并进入'}
            </button>
          </div>
        </form>
      </div>
    );
  }

  return (
    <div className="unlock-screen">
      <form className="unlock-card" onSubmit={(e) => void handleUnlock(e)}>
        <div className="unlock-brand">
          <span className="unlock-mark">
            <LockMark />
          </span>
          <h2 className="unlock-title">解锁保险库</h2>
          <p className="unlock-hint">输入主密码以解锁</p>
        </div>
        <input
          className="input"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder="主密码"
          autoFocus
          disabled={loading}
        />
        {error !== null && <div className="error">{error}</div>}
        <div className="unlock-actions">
          <button
            className="btn btn-primary btn-block"
            type="submit"
            disabled={loading || password.length === 0}
          >
            {loading && <span className="spinner" aria-hidden="true" />}
            {loading ? '解锁并同步中…' : '解锁'}
          </button>
        </div>
      </form>
    </div>
  );
}
