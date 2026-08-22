import { useEffect, useState } from 'react';
import { createVault, getSyncConfig, hasVault, setSyncConfig, syncNow, unlock } from '../lib/api';
import { useSession } from '../lib/session';

type Stage = 'loading' | 'config' | 'create' | 'unlock';

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
            <span className="unlock-kicker">初始设置</span>
            <h2 className="unlock-title">连接同步仓库</h2>
            <p className="unlock-hint">passm 通过 GitHub 私有仓库加密同步你的密码。</p>
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
          <p className="unlock-footnote">PAT 仅保存在系统密钥库中，不会写入仓库。</p>
        </form>
      </div>
    );
  }

  if (stage === 'create') {
    return (
      <div className="unlock-screen">
        <form className="unlock-card" onSubmit={(e) => void handleCreate(e)}>
          <div className="unlock-brand">
            <span className="unlock-kicker">初始化</span>
            <h2 className="unlock-title">创建保险库</h2>
            <p className="unlock-hint">设置主密码，请务必牢记——忘记将无法找回。</p>
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
          <span className="unlock-kicker">PASSM</span>
          <h2 className="unlock-title">解锁保险库</h2>
          <p className="unlock-hint">输入主密码以解锁。</p>
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
