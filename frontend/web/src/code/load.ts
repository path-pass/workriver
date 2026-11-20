// A unit's file texts: read when its code is shown, kept for the last few units, and their
// diff lines worked out when first read.
import type { FileChange, UnitCode } from './types'
import type { Unit } from '@/planning/types'
import { KEEP_CODES } from '@/config/defaults'
import { changeOf } from './files'

/**
 * A file with its texts. Its diff lines are worked out when first read, which is when its diff
 * is drawn: diffing every file of a big unit up front held its first showing for half a
 * second. The line counts are the service's (a minimal diff, as here); the sample has none.
 */
export const withLines = (f: FileChange): FileChange => {
  const before = f.before ?? null, after = f.after ?? null
  let lines: string[] | undefined
  const out = { ...f, before, after }
  if (f.add === undefined || f.del === undefined) { const c = changeOf(before, after); Object.assign(out, { add: c.add, del: c.del }); lines = c.lines }
  return Object.defineProperty(out, 'lines', { get: () => (lines ??= changeOf(before, after).lines), enumerable: true })
}

const fileKey = (commit: string, path: string) => `${commit}\0${path}`
// The units looked at last, each as last read: a unit's texts can be tens of MB, so only the
// last few are kept (going back and forth between two stays instant).
const codes = new Map<string, UnitCode>()

/**
 * A unit's file texts, read when its code is shown; read again only once its `code` changed,
 * and then only for commits not read before (a commit never changes) and uncommitted work.
 */
export async function loadCode(unit: Unit): Promise<UnitCode> {
  const had = codes.get(unit.id)
  // Read again or not, it is now the latest looked at.
  codes.delete(unit.id)
  if (had) codes.set(unit.id, had)
  if (had && had.code === (unit.code ?? '')) return had
  const have = unit.commits.filter(c => !c.live && had?.files.has(fileKey(c.id, c.files[0]?.path ?? ''))).map(c => c.id)
  const res = await fetch(`/api/unit/${encodeURIComponent(unit.id)}/code${have.length ? `?have=${have.join(',')}` : ''}`)
  if (!res.ok) throw new Error(`${res.status} ${await res.text()}`)
  const got: { code?: string; commits: { id: string; files: FileChange[] }[] } = await res.json()
  const kept = new Set(have)
  const files = new Map([...(had?.files ?? [])].filter(([k]) => kept.has(k.slice(0, k.indexOf('\0')))))
  for (const c of got.commits) for (const f of c.files) files.set(fileKey(c.id, f.path), withLines(f))
  const code = { code: got.code ?? '', files }
  codes.delete(unit.id)
  codes.set(unit.id, code)
  for (const old of [...codes.keys()].slice(0, -KEEP_CODES)) codes.delete(old)
  return code
}

/** The unit with its files' texts, where `code` has them. */
export const withCode = (unit: Unit, code: UnitCode): Unit =>
  ({ ...unit, commits: unit.commits.map(c => ({ ...c, files: c.files.map(f => code.files.get(fileKey(c.id, f.path)) ?? f) })) })
