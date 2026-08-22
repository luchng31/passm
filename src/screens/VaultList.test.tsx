import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Entry } from '../lib/api';
import { VaultList } from './VaultList';

vi.mock('../lib/api', () => ({
  list: vi.fn(),
  copy: vi.fn(),
  deleteEntry: vi.fn(),
  create: vi.fn(),
  update: vi.fn(),
  generatePassword: vi.fn(),
}));

import { deleteEntry, list } from '../lib/api';

function makeEntry(overrides: Partial<Entry>): Entry {
  return {
    id: '00000000-0000-0000-0000-000000000000',
    title: '',
    username: '',
    password: '',
    url: '',
    notes: '',
    version: 1,
    device_id: 'dev-1',
    created_at: 0,
    updated_at: 0,
    deleted: false,
    ...overrides,
  };
}

describe('VaultList', () => {
  beforeEach(() => {
    vi.mocked(list).mockResolvedValue([]);
  });

  it('renders entry titles as escaped text (XSS safety)', async () => {
    const evil = '<script>window.__pwned = true</script>';
    vi.mocked(list).mockResolvedValue([
      makeEntry({ id: 'id-1', title: evil, username: 'alice' }),
    ]);

    render(<VaultList />);

    const title = await screen.findByText(evil);
    expect(title).toBeInTheDocument();
    expect(document.querySelector('script')).toBeNull();
    expect((window as unknown as Record<string, unknown>).__pwned).toBeUndefined();
  });

  it('shows the empty state when there are no entries', async () => {
    render(<VaultList />);
    expect(await screen.findByText(/保险库为空/)).toBeInTheDocument();
  });

  it('opens the editor from the empty vault state (新建 must respond)', async () => {
    render(<VaultList />);
    // Wait for loading to finish first: capturing 新建 during the loading
    // frame grabs the toolbar node, which the racing load() then unmounts,
    // turning the click into a no-op on a detached element.
    await screen.findByText(/保险库为空/);
    fireEvent.click(screen.getByRole('button', { name: '新建' }));
    expect(await screen.findByText('新建条目')).toBeInTheDocument();
  });

  it('deletes an entry from the detail pane exactly once and removes the row', async () => {
    vi.mocked(list).mockResolvedValue([
      makeEntry({ id: 'id-1', title: 'GitHub', username: 'alice' }),
    ]);
    vi.mocked(deleteEntry).mockResolvedValue(makeEntry({ id: 'id-1' }));

    render(<VaultList />);

    fireEvent.click(await screen.findByText('GitHub'));
    fireEvent.click(await screen.findByRole('button', { name: '删除' }));
    fireEvent.click(screen.getByRole('button', { name: '确认' }));

    // regression guard: the delete side effect must fire exactly once —
    // a second call means the child AND parent both hit the backend, and
    // the parent's "条目不存在" failure skips the list-state update.
    await waitFor(() => expect(deleteEntry).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(screen.queryByText('GitHub')).not.toBeInTheDocument(),
    );
  });
});