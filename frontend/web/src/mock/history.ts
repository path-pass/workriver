// The sample's intent histories, kept in memory so the edit dialog and the history can be
// tried with VITE_MOCK=1: an intent's `history` in demo.json, else one version made of what
// it says now. The service rebuilds them from its records and refuses an edit from an older
// version by core's rule; this follows that rule as far as the sample needs.
import type { Intent, IntentText, IntentVersion } from '@/planning/types'

type Sampled = Intent & { history?: IntentVersion[] }
const kept = new Map<string, IntentVersion[]>()

function versions(i: Sampled): IntentVersion[] {
  let list = kept.get(i.id)
  if (!list) {
    list = i.history ? [...i.history] : [{ n: 1, at: '2026-11-01T09:00:00Z', by: 'user', how: 'created', title: i.title, goal: i.goal ?? '', note: i.note ?? '', ...(i.kind ? { kind: i.kind } : {}), ...(i.size ? { size: i.size } : {}), ...(i.tags?.length ? { tags: i.tags } : {}) }]
    kept.set(i.id, list)
  }
  return list
}

/** The text version `n` left, folded from the versions before it. */
function textAt(list: IntentVersion[], n: number): IntentText | null {
  if (n < 1 || n > list.length) return null
  const t: IntentText = { title: '', goal: '', note: '', kind: null, size: null, tags: [] }
  for (const v of list.slice(0, n)) {
    if (v.kind !== undefined) t.kind = v.kind
    if (v.size !== undefined) t.size = v.size
    if (v.tags !== undefined) t.tags = v.tags
    if (v.title != null) t.title = v.title
    if (v.goal != null) t.goal = v.goal
    if (v.note != null) t.note = v.note
    if (v.added != null) t.note = t.note.trimEnd() ? `${t.note.trimEnd()}\n\n${v.added}` : v.added
  }
  return t
}

export const mockHistory = (i: Intent) => versions(i)

export function mockVersion(i: Intent, n: number): { text: IntentText; before: IntentText | null } {
  const list = versions(i)
  const text = textAt(list, n)
  if (!text) throw new Error(`${i.id} has no version ${n}`)
  return { text, before: textAt(list, n - 1) }
}

/** Records an edit by the user, or throws the service's refusal; the intent's new fields. */
export function mockEdit(i: Intent, body: Record<string, unknown>): Partial<Intent> {
  const list = versions(i)
  const last = list[list.length - 1]
  const base = Number(body.base)
  for (const field of ['title', 'goal', 'note', 'kind', 'size', 'tags'] as const) {
    if (body[field] === undefined) continue
    const k = [...list].reverse().find(v => v[field] !== undefined || (field === 'note' && v.added != null))?.n ?? 1
    if (k > base) throw new Error(`/api/append: 422 ${i.id}'s ${field} changed since v${base} (in v${k}; it is at v${last.n}, last by ${last.name ?? last.by} at ${last.at})`)
  }
  const at = new Date().toISOString().replace(/\.\d+Z$/, 'Z')
  const pick = (f: 'title' | 'goal' | 'note') => (typeof body[f] === 'string' ? (body[f] as string) : undefined)
  const sort = Object.fromEntries((['kind', 'size', 'tags'] as const).filter(f => body[f] !== undefined).map(f => [f, body[f]]))
  list.push({ n: last.n + 1, at, by: 'user', how: 'edited', title: pick('title'), goal: pick('goal'), note: pick('note'), ...sort })
  const now = textAt(list, list.length)!
  return { title: now.title, goal: now.goal, note: now.note, kind: now.kind ?? undefined, size: now.size ?? undefined, tags: now.tags, version: list.length, edited: { at, by: null } }
}
