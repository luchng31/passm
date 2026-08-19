import { invoke } from '@tauri-apps/api/core';

// ---------------------------------------------------------------------------
// Types mirroring the T12 backend contract (src-tauri/src/commands.rs and
// crates/passm-vault/src/lib.rs). Field names match the Rust serde output.
// ---------------------------------------------------------------------------

export interface Entry {
  id: string;
  title: string;
  username: string;
  password: string;
  url: string;
  notes: string;
  version: number;
  device_id: string;
  created_at: number;
  updated_at: number;
  deleted: boolean;
}

export interface EntryInput {
  title: string;
  username: string;
  password: string;
  url: string;
  notes: string;
}

export interface SessionStatus {
  unlocked: boolean;
  device_id: string;
}

export interface SyncStatus {
  pushed: boolean;
  pulled: boolean;
  merged: boolean;
  backup_created: string | null;
}

export interface SyncConfig {
  remote_url: string;
}

export type CopyField = 'password' | 'username' | 'url';

// ---------------------------------------------------------------------------
// Typed invoke wrappers. Tauri 2 maps camelCase JS args to snake_case Rust
// params automatically (e.g. remoteUrl -> remote_url). Errors serialize to
// Chinese strings from the backend (e.g. "密码错误") and reject the promise.
// ---------------------------------------------------------------------------

export function unlock(password: string): Promise<void> {
  return invoke('unlock', { password });
}

export function lock(): Promise<void> {
  return invoke('lock');
}

export function getSessionStatus(): Promise<SessionStatus> {
  return invoke('get_session_status');
}

export function list(): Promise<Entry[]> {
  return invoke('list');
}

export function get(id: string): Promise<Entry> {
  return invoke('get', { id });
}

export function create(input: EntryInput): Promise<Entry> {
  return invoke('create', { input });
}

export function update(id: string, input: EntryInput): Promise<Entry> {
  return invoke('update', { id, input });
}

// `delete` is a reserved word in JS/TS; the backend command is still "delete".
export function deleteEntry(id: string): Promise<Entry> {
  return invoke('delete', { id });
}

export function search(q: string): Promise<Entry[]> {
  return invoke('search', { q });
}

export function copy(field: CopyField, id: string): Promise<void> {
  return invoke('copy', { field, id });
}

export function generatePassword(length: number): Promise<string> {
  return invoke('generate_password', { length });
}

export function syncNow(): Promise<SyncStatus> {
  return invoke('sync_now');
}

export function getSyncConfig(): Promise<SyncConfig | null> {
  return invoke('get_sync_config');
}

export function setSyncConfig(remoteUrl: string, pat: string): Promise<void> {
  return invoke('set_sync_config', { remoteUrl, pat });
}

export function hasVault(): Promise<boolean> {
  return invoke('has_vault');
}

export function createVault(password: string): Promise<void> {
  return invoke('create_vault', { password });
}