// An agent's terminal on the page, read-only: a dock at the bottom of the main area (a sheet on
// a narrow screen), one per page. It streams only while it is shown: folded, the page hidden or
// closed ends the stream; shown again opens a new one, whose first frame is the whole screen.
// xterm.js draws it (loaded when a view first opens, DOM renderer): no key reaches the agent,
// the agent's terminal is never resized (the font scales so its width fits, down to 9 px, then it scrolls
// sideways), nothing is written to the clipboard and a link in it does nothing.
import { useCallback, useEffect, useRef, useState } from 'react'
import type { Terminal } from '@xterm/xterm'
import { Button } from '@/ui/primitives/button'
import { Sheet, SheetContent, SheetTitle } from '@/ui/primitives/sheet'
import { useT } from '@/i18n'
import { focusAgent } from '@/api/actions'
import { viewTerminal, type Frame, type Place } from '@/api/terminal'
import { useResize } from '@/lib/useResize'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { Who } from './Mark'
import { Answer, placeOf, useScreen } from './Gate'
import type { Session } from './types'

/** The smallest and largest font the view scales to. */
const FONT = { min: 9, max: 14 }
const FAMILY = 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace'

/** How wide a cell is for each pixel of font size, measured once. */
let ratio = 0
function cellRatio() {
  if (ratio) return ratio
  try {
    const c = document.createElement('canvas').getContext('2d')
    if (c) { c.font = `100px ${FAMILY}`; ratio = c.measureText('W').width / 100 }
  } catch { /* measured below by a guess */ }
  return (ratio ||= 0.6)
}

const bytesOf = (b64: string) => Uint8Array.from(atob(b64), c => c.charCodeAt(0))

/** Whether the page is shown (not a background tab, not minimised). */
function useVisible() {
  const [visible, setVisible] = useState(() => document.visibilityState !== 'hidden')
  useEffect(() => {
    const on = () => setVisible(document.visibilityState !== 'hidden')
    document.addEventListener('visibilitychange', on)
    return () => document.removeEventListener('visibilitychange', on)
  }, [])
  return visible
}

/** Narrow screens show the view as a sheet. */
function useNarrow() {
  const query = '(width < 860px)'
  const [narrow, setNarrow] = useState(() => window.matchMedia?.(query).matches ?? false)
  useEffect(() => {
    const m = window.matchMedia?.(query)
    if (!m) return
    const on = () => setNarrow(m.matches)
    m.addEventListener('change', on)
    return () => m.removeEventListener('change', on)
  }, [])
  return narrow
}

/** How a stream ended for good: the service's reason, and whether any frame came. */
export interface Ended { reason?: string; framed: boolean }

/**
 * The view's frames while `active`, each handed to `onFrame`. A stream that ends (the
 * connection, a reader too slow) is reopened once, from a whole frame; after that, `ended`.
 * Within one stream a frame is drawn only after a whole one came on it, and only if its seq is
 * above the last drawn: a delta before any whole frame, or out of order, ends that stream as
 * an 'end' does (never drawn on a blank or old screen).
 */
export function useStream(place: Place, active: boolean, onFrame: (f: Frame) => void) {
  const [ended, setEnded] = useState<Ended | null>(null)
  const frame = useRef(onFrame)
  frame.current = onFrame
  const at = useRef(place)
  at.current = place
  const id = `${place.session}\n${place.machine ?? ''}`
  useEffect(() => {
    if (!active) return
    setEnded(null)
    let es: EventSource | null = null
    let reopened = false
    let framed = false
    let reason: string | undefined
    const stop = () => {
      es?.close()
      es = null
      if (!reopened) { reopened = true; open(); return }
      setEnded({ reason, framed })
    }
    const open = () => {
      const s = viewTerminal(at.current)
      es = s
      // The seq last drawn from this stream; none until a whole frame came.
      let last: number | null = null
      s.addEventListener('frame', e => {
        if (es !== s) return
        const f: Frame = JSON.parse((e as MessageEvent<string>).data)
        if ((last === null && !f.full) || (last !== null && !(f.seq > last))) return stop()
        last = f.seq
        framed = true
        frame.current(f)
      })
      s.addEventListener('end', e => {
        if (es !== s) return
        try { reason = JSON.parse((e as MessageEvent<string>).data).reason || reason } catch { /* no reason given */ }
        stop()
      })
      // EventSource would reconnect by itself; the page decides instead.
      s.onerror = () => { if (es === s) stop() }
    }
    open()
    return () => { es?.close(); es = null }
  }, [active, id])
  return ended
}

/** The xterm.js terminal in `host`, loaded on first use; frames that come before it are kept. */
function useXterm() {
  const host = useRef<HTMLDivElement | null>(null)
  const term = useRef<Terminal | null>(null)
  const queue = useRef<Frame[]>([])
  const fit = useCallback(() => {
    const t = term.current, h = host.current
    if (!t || !h) return
    const size = Math.max(FONT.min, Math.min(FONT.max, Math.floor(h.clientWidth / (t.cols * cellRatio()))))
    if (Number.isFinite(size) && t.options.fontSize !== size) t.options.fontSize = size
  }, [])
  const draw = useCallback((t: Terminal, f: Frame) => {
    if (f.full) t.reset()
    if (t.cols !== f.cols || t.rows !== f.rows) t.resize(f.cols, f.rows)
    t.write(bytesOf(f.bytes))
    fit()
  }, [fit])
  useEffect(() => {
    let dead = false
    let watch: ResizeObserver | undefined
    Promise.all([import('@xterm/xterm'), import('@xterm/xterm/css/xterm.css')]).then(([{ Terminal }]) => {
      if (dead || !host.current) return
      const t = new Terminal({
        disableStdin: true,
        cursorBlink: false,
        scrollback: 0,
        fontFamily: FAMILY,
        fontSize: FONT.max,
        // An OSC 8 link in what it draws: a click does nothing and asks nothing.
        linkHandler: { activate: () => {}, allowNonHttpProtocols: false },
      })
      // OSC 52 (write to the clipboard) is swallowed: nothing it draws reaches the clipboard.
      t.parser.registerOscHandler(52, () => true)
      t.open(host.current)
      term.current = t
      for (const f of queue.current.splice(0)) draw(t, f)
      if (typeof ResizeObserver !== 'undefined') { watch = new ResizeObserver(fit); watch.observe(host.current) }
    }, e => console.warn('terminal view:', String(e)))
    return () => { dead = true; watch?.disconnect(); term.current?.dispose(); term.current = null }
  }, [draw, fit])
  const write = useCallback((f: Frame) => {
    const t = term.current
    if (t) return draw(t, f)
    // A whole frame makes what came before it moot.
    if (f.full) queue.current = []
    queue.current.push(f)
  }, [draw])
  return { host, write }
}

/**
 * The gate under the view, while its agent waits: the screen its buttons are bound to and the
 * buttons, as in the gate (the stream above is not that screen: it may be blank, late or show
 * another dialog). It scrolls within its share of the dock.
 */
function DockAnswer({ s }: { s: Session }) {
  const screen = useScreen(placeOf(s))
  return <div className="max-h-[45%] flex-none overflow-y-auto border-t border-line p-3"><Answer screen={screen} /></div>
}

/** The terminal, the line why it is gone, and the gate's buttons while its agent waits. */
function Body({ s, gone, active }: { s: Session; gone: boolean; active: boolean }) {
  const { t } = useT()
  const { host, write } = useXterm()
  const ended = useStream(placeOf(s), active && !gone, write)
  const why = gone ? t('view.gone')
    : ended ? (ended.reason ? t('view.ended', { reason: ended.reason }) : ended.framed ? t('view.endedQuietly') : t('view.failed'))
    : null
  return (
    <>
      {why && <p role="status" className="flex-none px-3 py-1 text-xs text-ink-2 wrap-anywhere">{why}</p>}
      <div ref={host} className="term-view min-h-0 flex-1" data-gone={why ? '' : undefined} />
      {!gone && s.blockedSince && <DockAnswer s={s} />}
    </>
  )
}

/** Its state in a word, as Agents says it. */
function stateWord(s: Session, gone: boolean) {
  if (gone || s.ended) return 'gone' as const
  if (s.blockedSince) return 'blocked' as const
  return s.state === 'working' ? 'working' as const : s.state === 'idle' || s.state === 'done' ? 'idle' as const : null
}

/**
 * The terminal view of one agent. `gone`: it is no longer among the live sessions; its last
 * frame stays, greyed, with the line why.
 */
export function TerminalDock({ s, gone, onClose }: { s: Session; gone: boolean; onClose: () => void }) {
  const { t } = useT()
  const narrow = useNarrow()
  const visible = useVisible()
  const [folded, setFolded] = useState(false)
  const height = useResize({ key: 'workriver.view.height', variable: '--view-h', def: 320, min: 140, max: () => Math.round(window.innerHeight * 0.85), edge: 'top' })
  const word = stateWord(s, gone)
  const head = (
    <div className="flex flex-none items-center gap-2 px-3 py-1.5 text-sm">
      <Who name={s.agent} topic={s.topic} />
      <span className="min-w-0 truncate font-[550]">{s.agent}</span>
      {word && <span className="flex-none text-xs text-ink-2">{t(`agents.state.${word}`)}</span>}
      <span className="ml-auto flex flex-none items-center gap-1">
        <Button variant="ghost" size="xs" className="rounded-sm" onClick={() => focusAgent(s.session ?? s.agent)}>
          <Icon name="terminal" className="size-3.5" />{t('view.app')}
        </Button>
        {!narrow && (
          <Tip text={t(folded ? 'view.unfold' : 'view.fold')}>
            <Button variant="ghost" size="icon-xs" aria-label={t(folded ? 'view.unfold' : 'view.fold')} aria-expanded={!folded} onClick={() => setFolded(f => !f)}>
              <Icon name="chev" className={folded ? 'size-3.5 -rotate-90' : 'size-3.5 rotate-90'} />
            </Button>
          </Tip>
        )}
        <Tip text={t('view.close')}>
          <Button variant="ghost" size="icon-xs" aria-label={t('view.close')} onClick={onClose}><Icon name="no" className="size-3.5" /></Button>
        </Tip>
      </span>
    </div>
  )
  // Keyed by the agent: another agent's view starts from nothing.
  const key = `${s.session ?? s.agent}\n${s.machine ?? ''}`
  if (narrow)
    return (
      <Sheet open onOpenChange={o => { if (!o) onClose() }}>
        <SheetContent aria-describedby={undefined} className="w-screen">
          <SheetTitle className="sr-only">{t('view.title', { agent: s.agent })}</SheetTitle>
          {head}
          <Body key={key} s={s} gone={gone} active={visible} />
        </SheetContent>
      </Sheet>
    )
  return (
    <section
      aria-label={t('view.title', { agent: s.agent })}
      className="sticky bottom-0 z-20 -mx-6 -mb-10 mt-auto flex flex-col border-t border-line bg-surface data-[folded]:h-auto"
      style={folded ? undefined : { height: 'var(--view-h, 320px)' }}
      data-folded={folded || undefined}
    >
      {!folded && <div {...height.handle} aria-label={t('view.resize')} className="edge" data-edge="top" />}
      {head}
      {!folded && <Body key={key} s={s} gone={gone} active={visible} />}
    </section>
  )
}
