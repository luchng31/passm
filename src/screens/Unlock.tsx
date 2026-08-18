import { useState } from 'react';
import { unlock } from '../lib/api';
import { useSession } from '../lib/session';

export function Unlock() {
  const { setUnlocked } = useSession();
  const [password, setPassword] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (event: React.FormEvent) => {
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

  return (
    <div className="unlock-screen">
      <form className="unlock-card" onSubmit={(e) => void handleSubmit(e)}>
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