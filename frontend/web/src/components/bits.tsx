// Small parts used by every layer: the icon set, a tooltip, and an agent's mark.
import { createContext, useContext, useEffect, useState } from 'react'
import { Button } from '@/ui/primitives/button'
import { contentLang, useT } from '@/i18n'
import { cn } from '@/lib/utils'
import { focusAgent, NOTE, type Remote } from '@/api/actions'
import type { Step } from '@/work/types'
import type { Unit } from '@/planning/types'
import { Tip } from '@/ui/Tip'
import { Icon } from '@/ui/Icon'
import { CopyButton } from '@/ui/CopyButton'

/**
 * What an agent prepared for the user to do by hand: the same for a gate and for a chore.
 * With `onRun`, each command also has "Run", which asks to run it as the user.
 */
export function Steps({ steps, onRun }: { steps: Step[]; onRun?: (command: string, step: number) => void }) {
  const { t } = useT()
  return (
    <ol className="steps">
      {steps.map((s, k) => (
        <li key={k}>
          <span className="whitespace-pre-line" lang={contentLang(s.text)}>{s.text}</span>
          {s.command && (
            <div className="cmd">
              <code>{s.command}</code>
              <CopyButton text={s.command} />
              {onRun && (
                <Tip text={t('run.buttonFor', { command: s.command })}>
                  <Button variant="ghost" size="xs" className="flex-none rounded-sm" aria-label={t('run.buttonFor', { command: s.command })} onClick={() => onRun(s.command!, k + 1)}>
                    <Icon name="play" className="size-3" />{t('run.button')}
                  </Button>
                </Tip>
              )}
            </div>
          )}
        </li>
      ))}
    </ol>
  )
}

// Names one letter apart land far apart on the colour wheel.
const hue = (name: string) => [...name].reduce((n, ch) => (n * 31 + ch.charCodeAt(0) * 97) % 360, 7)

/** What each live agent's session is about, by name, for the marks' tooltips. */
export const Topics = createContext<Map<string, string>>(new Map())

/** Two letters for a name; a numbered one ("claude 2") keeps its number: "C2". */
export function initials(name: string) {
  const words = name.split(/[-_\s]+/).filter(Boolean)
  const last = words.at(-1) ?? ''
  if (words.length > 1 && /^\d+$/.test(last)) return (words[0][0] + last).slice(0, 3).toUpperCase()
  return words.map(w => w[0]).join('').slice(0, 2).toUpperCase()
}

/** An agent as a small coloured mark, the same wherever it shows up. */
export function Who({ name, live = false, topic: given }: { name: string; live?: boolean; topic?: string }) {
  const { t } = useT()
  const topics = useContext(Topics)
  const topic = given ?? topics.get(name)
  const label = topic ? `${name} · ${topic}` : name
  return (
    <Tip text={live ? t('who.writing', { name: label }) : label}>
      <span className="who" data-live={live || undefined} style={{ background: `hsl(${hue(name)} 55% 40%)` }}>
        {initials(name)}
      </span>
    </Tip>
  )
}

/** Opens the agent's terminal. A click target of its own, so it never also picks a row. */
export function TerminalButton({ agent, session, className }: { agent: string; session?: string | null; className?: string }) {
  const { t } = useT()
  return (
    <Tip text={t('terminal.open')}>
      <Button
        variant="ghost"
        size="icon-xs"
        className={cn('flex-none', className)}
        aria-label={t('terminal.openFor', { agent })}
        onClick={e => { e.stopPropagation(); focusAgent(session ?? agent) }}
      >
        <Icon name="terminal" className="size-3.5" />
      </Button>
    </Tip>
  )
}

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

/** The machine something is on, when it is not this one: a small chip with its name. */
export function MachineChip({ machine, away = false, since }: { machine?: string; away?: boolean; since?: string }) {
  const { t, date } = useT()
  if (!machine) return null
  const why = since ? t('machines.disconnected', { machine, time: date(since) }) : t('machines.unreachable', { machine })
  return (
    <Tip text={away ? why : t('machines.on', { machine })}>
      <span className="ext-chip" data-away={away || undefined}><Icon name="machine" className="size-3" /><span className="chip-name" lang={contentLang(machine)}>{machine}</span></span>
    </Tip>
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

