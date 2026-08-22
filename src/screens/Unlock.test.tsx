import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SessionContext } from '../lib/session';
import { Unlock } from './Unlock';

vi.mock('../lib/api', () => ({
  getSyncConfig: vi.fn(),
  setSyncConfig: vi.fn(),
  hasVault: vi.fn(),
  createVault: vi.fn(),
  unlock: vi.fn(),
  syncNow: vi.fn(),
}));

import { createVault, getSyncConfig, hasVault, setSyncConfig, unlock } from '../lib/api';

const sessionValue = {
  unlocked: false,
  deviceId: '',
  setUnlocked: vi.fn(),
  refreshTick: 0,
  bumpRefresh: vi.fn(),
};

function renderUnlock() {
  return render(
    <SessionContext.Provider value={sessionValue}>
      <Unlock />
    </SessionContext.Provider>,
  );
}

describe('Unlock', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getSyncConfig).mockResolvedValue(null);
    vi.mocked(hasVault).mockResolvedValue(false);
  });

  it('shows the sync config form when sync is not configured', async () => {
    renderUnlock();
    expect(await screen.findByText('连接同步仓库')).toBeInTheDocument();
    expect(screen.getByPlaceholderText(/GitHub 仓库地址/)).toBeInTheDocument();
    expect(screen.getByPlaceholderText(/个人访问令牌/)).toBeInTheDocument();
  });

  it('shows the create-vault form when sync is configured but no vault exists', async () => {
    vi.mocked(getSyncConfig).mockResolvedValue({ remote_url: 'https://github.com/a/v.git' });
    vi.mocked(hasVault).mockResolvedValue(false);
    renderUnlock();
    expect(await screen.findByText('创建保险库')).toBeInTheDocument();
  });

  it('shows the unlock form when a vault already exists', async () => {
    vi.mocked(getSyncConfig).mockResolvedValue({ remote_url: 'https://github.com/a/v.git' });
    vi.mocked(hasVault).mockResolvedValue(true);
    renderUnlock();
    expect(await screen.findByText('解锁保险库')).toBeInTheDocument();
  });

  it('moves from config to create-vault after saving config on an empty repo', async () => {
    vi.mocked(setSyncConfig).mockResolvedValue(undefined);
    vi.mocked(hasVault).mockResolvedValue(false);
    renderUnlock();

    fireEvent.change(await screen.findByPlaceholderText(/GitHub 仓库地址/), {
      target: { value: 'https://github.com/me/vault.git' },
    });
    fireEvent.change(screen.getByPlaceholderText(/个人访问令牌/), {
      target: { value: 'ghp_test' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));

    expect(setSyncConfig).toHaveBeenCalledWith('https://github.com/me/vault.git', 'ghp_test');
    expect(await screen.findByText('创建保险库')).toBeInTheDocument();
  });

  it('creates the vault and unlocks the session', async () => {
    vi.mocked(getSyncConfig).mockResolvedValue({ remote_url: 'https://github.com/a/v.git' });
    vi.mocked(hasVault).mockResolvedValue(false);
    vi.mocked(createVault).mockResolvedValue(undefined);
    renderUnlock();

    fireEvent.change(await screen.findByPlaceholderText('主密码'), {
      target: { value: 's3cret' },
    });
    fireEvent.change(screen.getByPlaceholderText('确认主密码'), {
      target: { value: 's3cret' },
    });
    fireEvent.click(screen.getByRole('button', { name: '创建并进入' }));

    expect(createVault).toHaveBeenCalledWith('s3cret');
    await waitFor(() => expect(sessionValue.setUnlocked).toHaveBeenCalledWith(true));
  });

  it('rejects mismatched passwords in the create form', async () => {
    vi.mocked(getSyncConfig).mockResolvedValue({ remote_url: 'https://github.com/a/v.git' });
    vi.mocked(hasVault).mockResolvedValue(false);
    renderUnlock();

    fireEvent.change(await screen.findByPlaceholderText('主密码'), {
      target: { value: 'aaa' },
    });
    fireEvent.change(screen.getByPlaceholderText('确认主密码'), {
      target: { value: 'bbb' },
    });
    fireEvent.click(screen.getByRole('button', { name: '创建并进入' }));

    expect(createVault).not.toHaveBeenCalled();
    expect(await screen.findByText('两次输入的密码不一致')).toBeInTheDocument();
  });

  it('unlocks with the master password when a vault exists', async () => {
    vi.mocked(getSyncConfig).mockResolvedValue({ remote_url: 'https://github.com/a/v.git' });
    vi.mocked(hasVault).mockResolvedValue(true);
    vi.mocked(unlock).mockResolvedValue(undefined);
    renderUnlock();

    fireEvent.change(await screen.findByPlaceholderText('主密码'), {
      target: { value: 'pw' },
    });
    fireEvent.click(screen.getByRole('button', { name: '解锁' }));

    expect(unlock).toHaveBeenCalledWith('pw');
    await waitFor(() => expect(sessionValue.setUnlocked).toHaveBeenCalledWith(true));
  });
});
