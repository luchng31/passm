import { useEffect, useState } from 'react';
import { createVault, getSyncConfig, hasVault, setSyncConfig, unlock } from '../lib/api';
import { useSession } from '../lib/session';

type Stage = 'loading' | 'config' | 'create' | 'unlock';

export function Unlock() {
  const { setUnlocked } = useSession();
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
          <h2>首次使用</h2>
          <p className="unlock-hint">配置同步仓库以开始使用</p>
          <input
            className="input"
            type="text"
            value={remoteUrl}
            onChange={(e) => setRemoteUrl(e.target.value)}
            placeholder="GitHub 仓库地址（https://github.com/…/vault.git）"
            autoFocus
            disabled={loading}
          />
          <input
            className="input"
            type="password"
            value={pat}
            onChange={(e) => setPat(e.target.value)}
            placeholder="GitHub 个人访问令牌 (PAT)"
            disabled={loading}
          />
          {error !== null && <div className="error">{error}</div>}
          <button
            className="btn btn-primary btn-block"
            type="submit"
            disabled={loading || remoteUrl.trim().length === 0 || pat.length === 0}
          >
            {loading ? '保存中…' : '保存配置'}
          </button>
          <p className="unlock-hint">PAT 仅保存在系统密钥库中，不会写入仓库</p>
        </form>
      </div>
    );
  }

  if (stage === 'create') {
    return (
      <div className="unlock-screen">
        <form className="unlock-card" onSubmit={(e) => void handleCreate(e)}>
          <h2>创建保险库</h2>
          <p className="unlock-hint">设置主密码（请务必牢记，忘记无法找回）</p>
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
          <button
            className="btn btn-primary btn-block"
            type="submit"
            disabled={loading || password.length === 0 || passwordConfirm.length === 0}
          >
            {loading ? '创建中…' : '创建并进入'}
          </button>
        </form>
      </div>
    );
  }

  return (
    <div className="unlock-screen">
      <form className="unlock-card" onSubmit={(e) => void handleUnlock(e)}>
        <h2>解锁保险库</h2>
        <p className="unlock-hint">输入主密码以解锁</p>
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
        <button
          className="btn btn-primary btn-block"
          type="submit"
          disabled={loading || password.length === 0}
        >
          {loading ? '解锁中…' : '解锁'}
        </button>
      </form>
    </div>
  );
}
