import type { Entry } from './api';

/**
 * Client-side search filter over title/username/url.
 * Case-insensitive, whitespace-separated multi-term AND semantics —
 * mirrors the backend `search_entries` behavior so the list view can
 * filter instantly without an invoke round-trip.
 */
export function filterEntries(entries: Entry[], query: string): Entry[] {
  const terms = query
    .trim()
    .toLowerCase()
    .split(/\s+/)
    .filter((term) => term.length > 0);
  if (terms.length === 0) {
    return entries;
  }
  return entries.filter((entry) =>
    terms.every(
      (term) =>
        entry.title.toLowerCase().includes(term) ||
        entry.username.toLowerCase().includes(term) ||
        entry.url.toLowerCase().includes(term)
    )
  );
}