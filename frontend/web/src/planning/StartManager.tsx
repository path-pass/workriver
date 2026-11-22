// Starting a manager from the page, the user's act: a harness and a model, in the project's
// folder, seated once it has a session (the service does the rest, off the request). The part
// of the manager select that does it, the controls Settings shares for the default, and the
// workspace's line while a start is pending or after it failed.
import { useContext, useEffect, useRef, useState } from 'react'
import { Button } from '@/ui/primitives/button'
import { Input } from '@/ui/primitives/input'
import { useT, type T } from '@/i18n'
import { SeatChanged, setSpace, startSettings } from '@/api/actions'
import { AgentContext } from '@/agents/context'
import type { HarnessOffer, Space, StartEnd, StartSettings } from '@/agents/types'
import type { Workspace } from '@/workspace/types'
import { Choice } from '@/ui/Choice'
import { Content, Slot, SLOT } from '@/content/Chips'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { cn } from '@/lib/utils'
import { replaces, runs } from './spaces'

/** A model as a start takes it (the service checks the same): empty, the harness's default; or
 *  one word of ASCII letters, digits and `. _ : / @ [ ] -`, at most 100, not starting with `-`. */
export const modelOk = (m: string) => m === '' || (m.length <= 100 && !m.startsWith('-') && /^[A-Za-z0-9._:/@[\]-]+$/.test(m))

const NAMES = ['claude', 'codex'] as const
/** A harness's name for people; one the page does not know, by its kind. */
export const harnessName = (t: T, kind: string) => ((NAMES as readonly string[]).includes(kind) ? t(`launch.names.${kind as (typeof NAMES)[number]}`) : kind)

const REASONS: StartEnd[] = ['runtime', 'no_agent', 'no_session', 'refused', 'restarted']

/** The harness a start takes first: the default when it is offered, else the first offered. */
export function firstHarness(s: StartSettings): string {
  const offered = s.harnesses.filter(h => !h.why)
  return offered.some(h => h.kind === s.harness) ? s.harness : offered[0]?.kind ?? s.harness
}

/**
 * The harness (a select; one not offered here is greyed, saying why) and the model (free text,
 * one word; empty is the user's default; the harness's own names for models as quick picks).
 * Shared by the start part of the manager select and Settings' default.
 */
export function HarnessFields({ offers, harness, model, onHarness, onModel }: {
  offers: HarnessOffer[]; harness: string; model: string; onHarness: (k: string) => void; onModel: (m: string) => void
}) {
  const { t } = useT()
  const offer = offers.find(o => o.kind === harness)
  const options = offers.map(o => {
    const name = harnessName(t, o.kind)
    return { value: o.kind, label: o.why ? t('launch.unavailable', { harness: name, why: t(`launch.why.${o.why}`) }) : name, disabled: !!o.why }
  })
  const bad = !modelOk(model)
  return (
    <div className="flex flex-col gap-2">
      <label className="flex items-center gap-2 text-xs text-ink-2">
        <span className="w-16 flex-none">{t('launch.harness')}</span>
        <Choice label={t('launch.harness')} value={harness} onChange={onHarness} options={options} className="h-8 flex-1" />
      </label>
      {offers.every(o => o.why) && <p className="text-xs text-ink-2">{t('launch.none')}</p>}
      <label className="flex items-center gap-2 text-xs text-ink-2">
        <span className="w-16 flex-none">{t('launch.model')}</span>
        <Input
          value={model} onChange={e => onModel(e.target.value.trim())} placeholder={t('launch.modelPlaceholder')} aria-label={t('launch.model')}
          aria-invalid={bad || undefined} spellCheck={false} autoCapitalize="off" autoCorrect="off" className="h-8 flex-1 font-mono text-sm md:text-sm"
        />
      </label>
      {!!offer?.aliases.length && (
        <p className="flex flex-wrap items-center gap-1 pl-18 text-xs text-ink-2">
          <span>{t('launch.modelHint')}</span>
          {offer.aliases.map(a => (
            <Button key={a} type="button" variant="outline" size="xs" aria-label={t('launch.alias', { model: a })} aria-pressed={model === a} className="font-mono aria-pressed:bg-sunken aria-pressed:font-semibold" onClick={() => onModel(model === a ? '' : a)}>{a}</Button>
          ))}
        </p>
      )}
      {bad && <p role="alert" className="text-xs text-del">{t('launch.modelBad')}</p>}
    </div>
  )
}

/** The default and what is offered here, read once when the part shows. */
export function useStartSettings() {
  const [settings, setSettings] = useState<StartSettings | null>(null)
  const [unread, setUnread] = useState('')
  useEffect(() => { startSettings().then(setSettings, e => setUnread(e instanceof Error ? e.message : String(e))) }, [])
  return { settings, setSettings, unread }
}

/**
 * "Start a new one", the manager select's second part: the harness, the model (the default
 * from Settings), and "Start and seat". Over a manager that runs it asks once, in a line, as
 * seating does; the change names the seat the user was shown, so a seat that changed
 * meanwhile is refused and said so. It answers once the start is recorded; the workspace's
 * line follows it from there. `focus`: opened by "Start manager", it is scrolled to and its
 * button focused.
 */
export function StartPart({ space, focus, onDone }: { space: Space; focus?: boolean; onDone?: () => void }) {
  const { t } = useT()
  const { settings, unread } = useStartSettings()
  const [harness, setHarness] = useState('')
  const [model, setModel] = useState('')
  const [asked, setAsked] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const go = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    if (!settings) return
    const h = firstHarness(settings)
    setHarness(h)
    setModel(h === settings.harness ? settings.model : '')
  }, [settings])
  useEffect(() => { if (focus && settings) { go.current?.scrollIntoView?.({ block: 'nearest' }); go.current?.focus() } }, [focus, settings])
  const seated = space.manager
  const shown = `${seated?.session ?? ''}:${seated?.since ?? 0}`
  useEffect(() => setAsked(false), [shown])
  const over = runs(seated)
  const pending = !!space.start && !space.start.reason
  const pickHarness = (k: string) => { setHarness(k); setModel(k === settings?.harness ? settings.model : '') }
  const offered = !!settings?.harnesses.some(h => h.kind === harness && !h.why)
  const start = () => {
    if (over && !asked) return setAsked(true)
    setBusy(true)
    setSpace({ action: 'start', workspace: space.id, harness, model, replaces: replaces(space) }).then(
      () => { setBusy(false); setError(''); onDone?.() },
      e => { setBusy(false); setError(e instanceof SeatChanged ? t('seat.changed') : t('launch.failed', { error: e instanceof Error ? e.message : String(e) })) },
    )
  }
  return (
    <section aria-labelledby={`start-${space.id}`} className="flex flex-col gap-2 border-t border-line pt-3 text-sm">
      <h3 id={`start-${space.id}`} className="text-sm font-[650]">{t('launch.section')}</h3>
      <p className="text-xs text-ink-2">{t('launch.lead')}</p>
      {!settings
        ? unread ? <p role="alert" className="text-xs wrap-anywhere text-del">{t('settings.readFailed', { error: unread })}</p> : <p aria-live="polite" className="text-xs text-ink-2">{t('settings.loading')}</p>
        : <HarnessFields offers={settings.harnesses} harness={harness} model={model} onHarness={pickHarness} onModel={setModel} />}
      {pending && <p className="text-xs text-ink-2">{t('launch.pending')}</p>}
      {over && asked && <p role="alert" className="text-xs"><Slot text={t('launch.over', { old: SLOT })}><Content text={seated.name} /></Slot></p>}
      {error && <p role="alert" className="text-xs wrap-anywhere text-del">{error}</p>}
      <div className="flex justify-end">
        <Button ref={go} type="button" size="sm" variant={over ? 'outline' : 'default'} disabled={busy || pending || !offered || !modelOk(model)} onClick={start}>
          <Icon name="play" className="size-3.5" />{over && asked ? t('launch.goOver') : t('launch.go')}
        </Button>
      </div>
    </section>
  )
}

/**
 * A workspace's start as its line says it: pending, "starting a manager…"; its agent waiting
 * for a person in its terminal (found by the name it runs under, on this machine: it has no
 * session yet), the same words and the gate of a stuck agent on a click; ended, why in words
 * with the runtime's or the rules' own quietly. Nothing when there is no start.
 */
export function StartLine({ ws, space, tone = 'page' }: { ws: Workspace | null; space: Space; tone?: 'rail' | 'page' }) {
  const { t } = useT()
  const agents = useContext(AgentContext)
  const s = space.start
  if (!s) return null
  const quiet = tone === 'rail' ? 'text-rail-ink-2' : 'text-ink-2'
  if (!s.reason) {
    const blocked = ws?.sessions.find(x => x.agent === s.name && !x.ended && !!x.blockedSince && !x.machine)
    if (blocked) {
      return (
        <button
          type="button" onClick={() => agents.answer({ name: blocked.agent, session: blocked.session, machine: blocked.machine })}
          className="flex max-w-full items-center gap-1 self-start rounded-sm bg-hold-bg px-1.5 py-0.5 text-left text-[11px] font-semibold text-hold-ink hover:shadow"
        >
          <Icon name="terminal" className="size-3 flex-none" /><span className="min-w-0">{t('launch.waiting')}</span>
        </button>
      )
    }
    return <span role="status" className={cn('flex items-center gap-1 text-[11px] italic', quiet)}><Icon name="play" className="size-3 flex-none" />{t('launch.starting')}</span>
  }
  const why = REASONS.includes(s.reason) ? t(`launch.reason.${s.reason}`) : s.reason
  return (
    <span className="flex min-w-0 flex-col gap-0.5 text-[11px]">
      <span className={tone === 'rail' ? 'text-rail-ink' : 'text-ink'}>{t('launch.ended', { reason: why })}</span>
      {s.words && <Tip text={s.words}><span className={cn('line-clamp-2 font-mono wrap-anywhere opacity-80', quiet)}>{s.words}</span></Tip>}
    </span>
  )
}
