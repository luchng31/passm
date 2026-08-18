import { describe, expect, it } from 'vitest';
import type { Entry } from './api';
import { filterEntries } from './search';

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

const entries: Entry[] = [
  makeEntry({
    id: 'a',
    title: 'GitHub Login',
    username: 'alice@example.com',
    url: 'https://github.com',
  }),
  makeEntry({
    id: 'b',
    title: 'GitLab Login',
    username: 'bob@example.com',
    url: 'https://gitlab.com',
  }),
  makeEntry({
    id: 'c',
    title: 'AWS Console',
    username: 'alice',
    url: 'https://aws.amazon.com',
  }),
];

describe('filterEntries', () => {
  it('empty query returns all entries', () => {
    expect(filterEntries(entries, '')).toHaveLength(3);
  });

  it('whitespace-only query returns all entries', () => {
    expect(filterEntries(entries, '   ')).toHaveLength(3);
  });

  it('is case-insensitive over title, username and url', () => {
    expect(filterEntries(entries, 'GITHUB')).toHaveLength(1);
    expect(filterEntries(entries, 'github')).toHaveLength(1);
    expect(filterEntries(entries, 'ALICE')).toHaveLength(2);
    expect(filterEntries(entries, 'example.com')).toHaveLength(2);
  });

  it('multi-term query requires every term to match (AND)', () => {
    expect(filterEntries(entries, 'github alice')).toHaveLength(1);
    expect(filterEntries(entries, 'github bob')).toHaveLength(0);
    expect(filterEntries(entries, 'login')).toHaveLength(2);
  });

  it('terms may match different fields of the same entry', () => {
    expect(filterEntries(entries, 'aws alice')).toHaveLength(1);
  });

  it('returns empty array when nothing matches', () => {
    expect(filterEntries(entries, 'zzz')).toHaveLength(0);
  });
});