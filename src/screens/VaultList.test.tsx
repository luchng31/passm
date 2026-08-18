import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Entry } from '../lib/api';
import { VaultList } from './VaultList';

vi.mock('../lib/api', () => ({
  list: vi.fn(),
  copy: vi.fn(),
  deleteEntry: vi.fn(),
}));

import { list } from '../lib/api';

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
});