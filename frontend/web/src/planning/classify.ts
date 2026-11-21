// An intent's kind, size and tags on the page: the closed values in their order, tags as the
// service keeps them, and the rail's filter (what it keeps, the project's tags, what is
// remembered per project). The rules on who sets them and their limits are core's.
import type { Intent } from './types'

export const KINDS = ['feature', 'fix', 'chore', 'research'] as const
export type Kind = (typeof KINDS)[number]

export const SIZES = ['s', 'm', 'l'] as const
export type Size = (typeof SIZES)[number]

/** The most tags an intent carries (core's `MAX_TAGS`). */
export const MAX_TAGS = 8

/** Tags as the service keeps them: trimmed, lower-case, no repeats, none empty. */
export function normalTags(given: string[]): string[] {
  const out: string[] = []
  for (const t of given) {
    const x = t.trim().toLowerCase()
    if (x && !out.includes(x)) out.push(x)
  }
  return out
}

/** Typed text as tags: split at commas (and spaces, which a tag cannot hold). */
export const splitTags = (text: string) => normalTags(text.split(/[,\s]+/))

/** What the rail narrows to: within each of kind, size and tag any one ticked, all three together. */
export interface Filter {
  kinds: Kind[];
  sizes: Size[];
  tags: string[];
}

export const NO_FILTER: Filter = { kinds: [], sizes: [], tags: [] }

/** How many filters are on: the number the rail's filter button shows. */
export const filtersOn = (f: Filter) => f.kinds.length + f.sizes.length + f.tags.length

/** Whether an intent passes the filter. */
export function keeps(f: Filter, i: Intent): boolean {
  return (!f.kinds.length || (!!i.kind && f.kinds.includes(i.kind)))
    && (!f.sizes.length || (!!i.size && f.sizes.includes(i.size)))
    && (!f.tags.length || (i.tags ?? []).some(t => f.tags.includes(t)))
}

/** The tags of these intents, the most used first (then by name). */
export function tagsOf(intents: Intent[]): string[] {
  const n = new Map<string, number>()
  for (const i of intents) for (const t of i.tags ?? []) n.set(t, (n.get(t) ?? 0) + 1)
  return [...n.keys()].sort((a, b) => n.get(b)! - n.get(a)! || a.localeCompare(b))
}

const key = (project: string | null) => `workriver.filter.${project ?? '*'}`

/** The filter remembered for a project (null: all projects); none when nothing is. */
export function loadFilter(project: string | null): Filter {
  try {
    const v = JSON.parse(localStorage.getItem(key(project)) ?? 'null') as Partial<Filter> | null
    const list = <T extends string>(x: unknown, allowed?: readonly T[]) => (Array.isArray(x) ? x.filter((y): y is T => typeof y === 'string' && (!allowed || allowed.includes(y as T))) : [])
    return v ? { kinds: list(v.kinds, KINDS), sizes: list(v.sizes, SIZES), tags: list(v.tags) } : NO_FILTER
  } catch {
    return NO_FILTER
  }
}

export function saveFilter(project: string | null, f: Filter) {
  if (filtersOn(f)) localStorage.setItem(key(project), JSON.stringify(f))
  else localStorage.removeItem(key(project))
}
