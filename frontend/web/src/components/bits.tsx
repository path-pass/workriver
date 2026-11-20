// Small parts used by every layer: the icon set, a tooltip, and an agent's mark.
import { useEffect, useState } from 'react'
import { useT } from '@/i18n'
import { NOTE, type Remote } from '@/api/actions'
import type { Unit } from '@/planning/types'
import { Tip } from '@/ui/Tip'
import { CopyButton } from '@/ui/CopyButton'

/** A short note at the bottom of the screen: what the UI cannot do itself yet, or where a
 *  terminal brought to the front on another machine is (read longer). */
export function Notes() {
  const { t } = useT()
  const [text, setText] = useState<string | null>(null)
  useEffect(() => {
    let timer = 0
    const on = (e: Event) => {
      const d = (e as CustomEvent<{ agent?: string; remote?: Remote }>).detail
      const r = d.remote
      setText(r ? t('terminal.remote', { what: r.agent ?? t('terminal.command'), machine: r.machine, workspace: r.workspace ?? '?' }) : t('terminal.note', { agent: d.agent }))
      clearTimeout(timer)
      timer = window.setTimeout(() => setText(null), r ? 8000 : 2500)
    }
    window.addEventListener(NOTE, on)
    return () => { window.removeEventListener(NOTE, on); clearTimeout(timer) }
  }, [t])
  return (
    <div role="status" className="pointer-events-none fixed inset-x-0 bottom-4 z-50 flex justify-center px-4">
      {text && <div className="max-w-xl rounded-md bg-ink px-3 py-2 text-xs text-canvas shadow-pick">{text}</div>}
    </div>
  )
}

/**
 * Where a unit stands, as a small dot and a word: working, waiting, ready to merge (in the
 * done tone, so what can be merged stands out), merged, or done (with why, on hover).
 */
export function Progress({ unit }: { unit: Unit }) {
  const { t } = useT()
  const p = unit.progress ?? (unit.status === 'merged' ? 'merged' : null)
  if (!p) return null
  // Research and spikes are never merged: ready, they are closed.
  const tag = <span className="tag font-normal tracking-normal" data-progress={p}>{t(p === 'ready' && unit.kind ? 'unit.progress.readyClose' : `unit.progress.${p}`)}</span>
  return unit.doneReason ? <Tip text={unit.doneReason}>{tag}</Tip> : tag
}

/**
 * A ready unit whose every change is already in its base: merged by squash, most likely,
 * which git cannot show. Said next to "Ready to merge", with the command that records it.
 */
export function Squashed({ unit }: { unit: Unit }) {
  const { t } = useT()
  const s = unit.squash!
  const command = t('unit.mergedCommand', { unit: unit.id })
  return (
    <span className="inline-flex items-center gap-1 text-xs font-normal tracking-normal text-ink-2">
      <Tip text={t('unit.squashedWhy', { into: s.into, commit: s.commit.slice(0, 9) })}><span className="tag font-normal tracking-normal">{t('unit.squashed')}</span></Tip>
      <code className="font-mono">{command}</code>
      <CopyButton text={command} />
    </span>
  )
}

