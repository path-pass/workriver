// A unit with its files' texts, as the code layer shows it.
import { useEffect, useMemo, useRef, useState } from 'react'
import { MOCK } from '@/workspace/source'
import { noCode } from '@/planning/select'
import { loadCode, withCode } from './load'
import type { UnitCode } from './types'
import type { Unit } from '@/planning/types'

/**
 * The unit with its files' texts, for the code layer: read when it is shown, and again only
 * when its `code` changes. Until the new texts are there, the unit as they were read for.
 * Null until the first arrive. The sample has them inline.
 */
export function useCode(unit: Unit | null): Unit | null {
  const [got, setGot] = useState<{ unit: Unit; code: UnitCode } | null>(null)
  const want = unit && !MOCK && !noCode(unit) ? `${unit.id}\0${unit.code ?? ''}` : null
  const latest = useRef(unit)
  latest.current = unit
  useEffect(() => {
    const u = latest.current
    if (!want || !u) return
    let gone = false
    loadCode(u).then(code => { if (!gone) setGot({ unit: u, code }) }, e => console.warn(e))
    return () => { gone = true }
  }, [want])
  return useMemo(() => {
    if (!unit || MOCK || noCode(unit)) return unit
    if (got?.unit.id !== unit.id) return null
    return withCode(got.code.code === (unit.code ?? '') ? unit : got.unit, got.code)
  }, [unit, got])
}
