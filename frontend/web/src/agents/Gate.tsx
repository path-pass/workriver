// An agent waiting for the user in its terminal, answered from the page. The service reads
// its screen and parses the dialog on it; the page shows the screen as plain text in the order
// a terminal draws it, and offers the dialog's options, the keys it names and a few generic
// keys. A press is bound to the screen shown (its digest): the service sends nothing when its
// screen differs. The gate is a modal dialog; the same buttons sit under the terminal view.
import { useCallback, useEffect, useRef, useState } from 'react'
import { Button } from '@/ui/primitives/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/ui/primitives/collapsible'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/ui/primitives/dialog'
import { contentLang, useT, type T } from '@/i18n'
import { pressKeys, readScreen, type Place } from '@/api/terminal'
import { TerminalText } from '@/content/TerminalText'
import { NodeName } from '@/work/Nodes'
import { ms, unitsOf } from '@/planning/select'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { Who } from './Mark'
import { KEYS, type Dialog as Parsed, type Key, type Outcome, type Press, type Refusal, type ScreenView, type Session } from './types'
import type { Pick } from '@/work/types'
import type { Workspace } from '@/workspace/types'

/** Where a session is, for the routes. */
export const placeOf = (s: Session): Place => ({ session: s.session ?? s.agent, machine: s.machine })

/** The refusals the page has words for; any other is "failed". */
const CODES = ['gone', 'two', 'not-waiting', 'busy', 'changed', 'no-dialog', 'invalid', 'runtime', 'origin', 'views', 'refused', 'failed', 'unsure'] as const
type Code = (typeof CODES)[number]
const codeOf = (c: string): Code => ((CODES as readonly string[]).includes(c) ? (c as Code) : 'failed')
const asRefusal = (e: unknown): Refusal => (e && typeof e === 'object' && 'code' in e ? (e as Refusal) : { code: 'failed', reason: String(e) })

/** What a key looks like on its cap: an arrow, ↵, Esc, Tab, or the digit. */
export const cap = (t: T, k: string) => (/^[1-9]$/.test(k) ? k : (KEYS as string[]).includes(k) ? t(`screen.cap.${k as Exclude<Key, `${number}`>}`) : k)
const keyName = (t: T, k: Key) => (/^[1-9]$/.test(k) ? k : t(`screen.keyName.${k as Exclude<Key, `${number}`>}`))

/** The keys an option sends: its digit when numbered; else arrows from the marked one, then Enter. */
export function sends(d: Parsed, i: number): Key[] {
  if (d.numbered) return [String(d.options[i].number ?? i + 1) as Key]
  const m = Math.max(0, d.options.findIndex(o => o.marked))
  return [...Array<Key>(Math.abs(i - m)).fill(i > m ? 'down' : 'up'), 'enter']
}

/** What came of the last press: an outcome, or a refusal with the service's words. */
export type Said = { outcome: Outcome } | { refusal: Refusal }

/** The screen of one agent, read when it is asked for, and the presses on it. */
export function useScreen(place: Place | null, onAnswered?: () => void) {
  const [view, setView] = useState<ScreenView | null>(null)
  const [failed, setFailed] = useState<Refusal | null>(null)
  const [reading, setReading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [said, setSaid] = useState<Said | null>(null)
  const id = place && `${place.session}\n${place.machine ?? ''}`
  // Only the latest read counts; one for another agent, or answered after this one, is dropped.
  const seq = useRef(0)
  const answered = useRef(onAnswered)
  answered.current = onAnswered
  const at = useRef(place)
  at.current = place
  const load = useCallback(() => {
    const p = at.current
    if (!p) return
    const k = ++seq.current
    setReading(true)
    readScreen(p).then(
      v => { if (k === seq.current) { setView(v); setFailed(null) } },
      e => { if (k === seq.current) { setView(null); setFailed(asRefusal(e)) } },
    ).finally(() => { if (k === seq.current) setReading(false) })
  }, [])
  useEffect(() => {
    setView(null); setFailed(null); setSaid(null); setBusy(false)
    if (id) load()
    return () => { seq.current++ }
  }, [id, load])
  const press = async (pr: Press) => {
    const p = at.current
    if (!p || !view || busy) return
    setBusy(true)
    setSaid(null)
    const k = ++seq.current
    try {
      const a = await pressKeys(p, view.digest, pr)
      if (k !== seq.current) return
      setSaid({ outcome: a.outcome })
      if (a.outcome === 'answered') { setView(null); answered.current?.() }
      else if (a.screen) setView(a.screen)
      // Never sent again by itself: look again.
      else load()
    } catch (e) {
      if (k !== seq.current) return
      const r = asRefusal(e)
      setSaid({ refusal: r })
      if (r.screen) setView(r.screen)
      else if (r.code === 'unsure') load()
    } finally {
      setBusy(false)
    }
  }
  return { view, failed, reading, busy, said, load, press }
}
export type Screen = ReturnType<typeof useScreen>

/** A key cap: what a button will send. */
function Caps({ keys }: { keys: string[] }) {
  const { t } = useT()
  return <kbd className="flex-none rounded-sm border border-line-2 bg-sunken px-1 font-mono text-[11px] font-normal text-ink-2">{keys.map(k => cap(t, k)).join(' ')}</kbd>
}

/** The line about the last press, or why the screen could not be read. */
function SaidLine({ said }: { said: Said }) {
  const { t } = useT()
  if ('outcome' in said) return <p role="status" className="text-xs text-ink-2">{t(`screen.outcome.${said.outcome}`)}</p>
  const r = said.refusal
  return (
    <Tip text={r.reason || t(`screen.refused.${codeOf(r.code)}`)}>
      <p role="alert" tabIndex={0} className="text-xs text-del wrap-anywhere">{t(`screen.refused.${codeOf(r.code)}`)}</p>
    </Tip>
  )
}

/**
 * The screen and the buttons to answer it, in the gate and under the terminal view alike: the
 * buttons are shown with the screen they are bound to. Every press goes with its digest.
 */
export function Answer({ screen, onView }: { screen: Screen; onView?: () => void }) {
  const { t } = useT()
  const { view, failed, reading, busy, said, load, press } = screen
  const d = view?.dialog ?? null
  const lines = view ? view.text.split('\n') : []
  const off = busy || !view
  return (
    <div className="flex min-w-0 flex-col gap-3" aria-busy={busy || reading || undefined}>
      {reading && !view && !failed && <p className="text-xs text-ink-2"><span className="pending" aria-hidden />{t('screen.reading')}</p>}
      {failed && (
        <div className="flex flex-wrap items-center gap-2">
          <Tip text={failed.reason || t(`screen.refused.${codeOf(failed.code)}`)}>
            <p role="alert" tabIndex={0} className="text-xs text-del wrap-anywhere">{t('screen.readFailed', { reason: t(`screen.refused.${codeOf(failed.code)}`) })}</p>
          </Tip>
          <Button variant="outline" size="xs" className="rounded-sm" disabled={reading} onClick={load}>{t('screen.retry')}</Button>
        </div>
      )}
      {view && d && (
        <TerminalText block text={lines.slice(d.top, d.bottom + 1).join('\n')} className="quote-screen" />
      )}
      {view && (
        <Collapsible key={view.digest} defaultOpen={!d || d.cut}>
          <CollapsibleTrigger className="group/whole flex items-center gap-1 rounded-sm text-xs text-ink-2 hover:text-ink">
            <Icon name="chev" className="size-3.5 transition-transform group-data-[state=open]/whole:rotate-90" />{t('screen.whole')}
          </CollapsibleTrigger>
          {d?.cut && <p className="mt-1 text-xs text-hold-ink">{t('screen.cut')}</p>}
          <CollapsibleContent>
            <TerminalText block text={view.text} className="quote-screen mt-1 max-h-[50vh]" />
          </CollapsibleContent>
        </Collapsible>
      )}
      {view && !d && <p className="text-xs text-ink-2">{t('screen.noDialog')}</p>}
      {d && (
        <ul className="flex flex-col gap-1" aria-label={t('screen.options')}>
          {d.options.map((o, i) => (
            <li key={i}>
              <Button variant="outline" className="h-auto w-full justify-start gap-2 rounded-sm px-2.5 py-1.5 text-left whitespace-normal" disabled={off} onClick={() => press({ option: i + 1 })}>
                <span className="flex min-w-0 flex-1 flex-col items-start gap-0.5">
                  <TerminalText text={o.words} className="font-mono text-[13px] wrap-anywhere" />
                  {o.more.map((m, k) => <TerminalText key={k} text={m} className="font-mono text-xs font-normal text-ink-2 wrap-anywhere" />)}
                </span>
                <Caps keys={sends(d, i)} />
              </Button>
            </li>
          ))}
        </ul>
      )}
      {d && d.hints.length > 0 && (
        <div className="flex flex-wrap gap-1.5" aria-label={t('screen.hints')} role="group">
          {d.hints.map((h, k) => (
            <Button key={k} variant="outline" size="xs" className="rounded-sm" disabled={off} onClick={() => press({ key: h.key })}>
              <Caps keys={[h.key]} /><TerminalText text={h.words} className="font-mono font-normal" />
            </Button>
          ))}
        </div>
      )}
      {/* For doing it by hand: folded when the dialog was read, open when nothing was. */}
      <Collapsible key={d ? 'parsed' : 'none'} defaultOpen={!d}>
        <CollapsibleTrigger className="group/keys flex items-center gap-1 rounded-sm text-xs text-ink-2 hover:text-ink">
          <Icon name="chev" className="size-3.5 transition-transform group-data-[state=open]/keys:rotate-90" />{t('screen.other')}
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="mt-1 flex flex-wrap gap-1" role="group" aria-label={t('screen.other')}>
            {KEYS.map(k => (
              <Button key={k} variant="ghost" size="xs" className="min-w-7 rounded-sm border border-line font-mono" disabled={off} aria-label={t('screen.press', { key: keyName(t, k) })} onClick={() => press({ key: k })}>
                {cap(t, k)}
              </Button>
            ))}
          </div>
        </CollapsibleContent>
      </Collapsible>
      {said && <SaidLine said={said} />}
      {(onView || view) && (
        <div className="flex flex-wrap items-center gap-2">
          {view && <Button variant="ghost" size="xs" className="rounded-sm" disabled={busy || reading} onClick={load}>{t('screen.again')}</Button>}
          {onView && (
            <Button variant={d ? 'outline' : 'default'} size="xs" className="ml-auto rounded-sm" onClick={onView}>
              <Icon name="terminal" className="size-3.5" />{t('screen.openView')}
            </Button>
          )}
        </div>
      )}
    </div>
  )
}

/**
 * The gate: who waits, on what, for how long and why (quietly); then its screen and the
 * buttons. Answered, it closes; the Blocking row leaves by itself with the next snapshot.
 */
export function Gate({ ws, s, onClose, onOpen, onView }: { ws: Workspace; s: Session | null; onClose: () => void; onOpen: (unit: string, pick: Pick) => void; onView: (s: Session) => void }) {
  const { t, waited, when } = useT()
  const screen = useScreen(s && placeOf(s), onClose)
  const mine = (n: { agent?: string | null; agentSession?: string | null }) => (n.agentSession && s?.session ? n.agentSession === s.session : n.agent === s?.agent)
  const held = s ? unitsOf(ws).flatMap(u => u.tasks.filter(n => mine(n) && (n.state === 'running' || n.state === 'todo')).map(n => ({ u, n }))).sort((a, b) => Number(b.n.state === 'running') - Number(a.n.state === 'running'))[0] : undefined
  const since = ms(s?.blockedSince ?? screen.view?.since)
  const onScreen = screen.view ? screen.view.waiting === 'screen' : !!s?.onScreen
  return (
    <Dialog open={!!s} onOpenChange={o => { if (!o) onClose() }}>
      <DialogContent closeLabel={t('settings.close')} className="max-h-[90vh] overflow-y-auto sm:max-w-[960px]">
        {s && (
          <>
            <DialogHeader className="gap-1 pr-8">
              <DialogTitle className="flex min-w-0 items-center gap-2"><Who name={s.agent} topic={s.topic} /><span className="truncate">{t('inbox.blockedAgent', { agent: s.agent })}</span></DialogTitle>
              <DialogDescription className="flex flex-wrap items-center gap-x-2 text-xs text-ink-2">
                {held
                  ? <button type="button" className="min-w-0 truncate underline underline-offset-2 hover:text-ink" onClick={() => { onOpen(held.u.id, { kind: 'task', id: held.n.id }); onClose() }}><NodeName n={held.n} /></button>
                  : s.group && <span lang={contentLang(s.group.label)}>{s.group.label}</span>}
                {Number.isFinite(since) && <span title={when(since)}>{t('screen.waited', { time: waited(since) })}</span>}
              </DialogDescription>
              {onScreen
                ? <p className="text-xs text-ink-2">{t('screen.onScreen')}</p>
                : screen.view?.why && <TerminalText text={screen.view.why} className="font-mono text-xs text-ink-2 wrap-anywhere" />}
            </DialogHeader>
            <Answer screen={screen} onView={() => { onView(s); onClose() }} />
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}

