// Small parts used by every layer: the icon set, a tooltip, and an agent's mark.
import { Archive, ArrowLeft, Ban, FoldVertical, UnfoldVertical, EyeOff, Server, ArrowUp, Bot, ExternalLink, GripVertical, Play, Telescope, Crown, DraftingCompass, Scale, FlaskConicalOff, GitCommitHorizontal, Link2, Check, CircleDashed, ChevronRight, Circle, Clock, Copy, Expand, Eye, FileText, GitMerge, FlaskConical, Hand, ImagePlus, Inbox, ListTodo, MessageSquare, Moon, OctagonAlert, PanelLeftClose, PanelLeftOpen, PanelRightClose, Plus, SquareTerminal, Users, Pencil, Rocket, Shield, X, type LucideIcon } from 'lucide-react'
import { createContext, lazy, Suspense, useContext, useEffect, useLayoutEffect, useRef, useState, type ReactElement, type ReactNode } from 'react'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useT } from '@/i18n'
import { cn } from '@/lib/utils'
import { focusAgent, NOTE } from '@/runtime'
import type { External, Step, Unit } from '@/types'

// Who raised it is an icon; so is the state of a piece of work. All from lucide, one size, one stroke.
const ICONS: Record<string, LucideIcon> = {
  check: Check,
  ring: Circle,
  pen: Pencil,
  later: Clock,
  planned: CircleDashed,
  agent: Bot,
  manager: Crown,
  'gate-deploy': Rocket,
  'gate-test': Hand,
  'gate-decide': Scale,
  design: DraftingCompass,
  research: Telescope,
  backlog: Archive,
  machine: Server,
  blind: EyeOff,
  grip: GripVertical,
  play: Play,
  link: ExternalLink,
  blocked: Hand,
  blocker: OctagonAlert,
  chore: ListTodo,
  inbox: Inbox,
  agents: Users,
  fold: PanelRightClose,
  idle: Moon,
  copy: Copy,
  permalink: Link2,
  terminal: SquareTerminal,
  talk: MessageSquare,
  send: ArrowUp,
  attach: ImagePlus,
  file: FileText,
  plus: Plus,
  untested: FlaskConicalOff,
  dropped: Ban,
  commits: GitCommitHorizontal,
  rail: PanelLeftClose,
  'rail-open': PanelLeftOpen,
  no: X,
  'security-review': Shield,
  'code-review': Eye,
  test: FlaskConical,
  grew: Expand,
  chev: ChevronRight,
  merge: GitMerge,
  back: ArrowLeft,
  foldLines: FoldVertical,
  unfold: UnfoldVertical,
}

export function Icon({ name, className }: { name: string; className?: string }) {
  const Glyph = ICONS[name] ?? Circle
  return <Glyph aria-hidden size={16} strokeWidth={1.5} className={cn('ic', className)} />
}

export function Tip({ text, children }: { text: ReactNode; children: ReactElement }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent className="wrap-anywhere">{text}</TooltipContent>
    </Tooltip>
  )
}

/** An icon that says something by itself: named for assistive tech, explained on hover. */
export function Mark({ name, label }: { name: string; label: string }) {
  return (
    <Tip text={label}>
      <span role="img" aria-label={label} className="inline-flex flex-none"><Icon name={name} className="size-3.5" /></span>
    </Tip>
  )
}

/** How many findings of a review wait for the user: the one violet count, wherever it shows. */
export function Waiting({ count }: { count: number }) {
  const { t, num } = useT()
  return (
    <span className="count">
      <span aria-hidden>{num(count)}</span>
      <span className="sr-only">{t('finding.waiting', { count })}</span>
    </span>
  )
}

/**
 * Copies `text`; with a `label` it is a small text button, without, an icon (for a command,
 * or what `aria` names, which its tooltip says too).
 */
export function CopyButton({ text, label, aria, className }: { text: string; label?: string; aria?: string; className?: string }) {
  const { t } = useT()
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch { /* no clipboard here: the command can still be selected by hand */ }
  }
  if (label)
    return (
      <Button variant="ghost" size="xs" className="rounded-sm" onClick={copy}>
        <Icon name={copied ? 'check' : 'copy'} className="size-3" />{copied ? t('steps.copied') : label}
      </Button>
    )
  const icon = (
    <Button variant="ghost" size="icon-xs" className={cn('flex-none', className)} aria-label={copied ? t('steps.copied') : aria ?? t('steps.copy')} onClick={copy}>
      <Icon name={copied ? 'check' : aria ? 'permalink' : 'copy'} className="size-3.5" />
    </Button>
  )
  if (!aria) return icon
  // A changed name is not read out: the confirmation is said once, politely.
  return <><Tip text={copied ? t('steps.copied') : aria}>{icon}</Tip><span role="status" className="sr-only">{copied ? t('steps.copied') : ''}</span></>
}

/**
 * What an agent prepared for the user to do by hand: the same for a gate and for a chore.
 * With `onRun`, each command also has "Run", which asks to run it as the user.
 */
export function Steps({ steps, onRun }: { steps: Step[]; onRun?: (command: string) => void }) {
  const { t } = useT()
  return (
    <ol className="steps">
      {steps.map((s, k) => (
        <li key={k}>
          <span className="whitespace-pre-line">{s.text}</span>
          {s.command && (
            <div className="cmd">
              <code>{s.command}</code>
              <CopyButton text={s.command} />
              {onRun && (
                <Tip text={t('run.buttonFor', { command: s.command })}>
                  <Button variant="ghost" size="xs" className="flex-none rounded-sm" aria-label={t('run.buttonFor', { command: s.command })} onClick={() => onRun(s.command!)}>
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

/** A short note at the bottom of the screen for what the UI cannot do itself yet. */
export function Notes() {
  const { t } = useT()
  const [agent, setAgent] = useState<string | null>(null)
  useEffect(() => {
    let timer = 0
    const on = (e: Event) => {
      setAgent((e as CustomEvent<{ agent: string }>).detail.agent)
      clearTimeout(timer)
      timer = window.setTimeout(() => setAgent(null), 2500)
    }
    window.addEventListener(NOTE, on)
    return () => { window.removeEventListener(NOTE, on); clearTimeout(timer) }
  }, [])
  return (
    <div role="status" className="pointer-events-none fixed inset-x-0 bottom-4 z-50 flex justify-center px-4">
      {agent && <div className="rounded-md bg-ink px-3 py-2 text-xs text-canvas shadow-pick">{t('terminal.note', { agent })}</div>}
    </div>
  )
}

/** Where an item is tracked elsewhere (jira:ABC-12), as a small chip; a link when there is one. */
export function ExternalChip({ ext }: { ext?: External }) {
  if (!ext) return null
  const text = `${ext.system}:${ext.key}`
  return ext.url
    ? <a className="ext-chip" href={ext.url} target="_blank" rel="noreferrer noopener" onClick={e => e.stopPropagation()}><Icon name="link" className="size-3" /><ChipName name={text} /></a>
    : <span className="ext-chip"><ChipName name={text} /></span>
}

/** The machine something is on, when it is not this one: a small chip with its name. */
export function MachineChip({ machine, away = false, since }: { machine?: string; away?: boolean; since?: string }) {
  const { t, date } = useT()
  if (!machine) return null
  const why = since ? t('machines.disconnected', { machine, time: date(since) }) : t('machines.unreachable', { machine })
  return (
    <Tip text={away ? why : t('machines.on', { machine })}>
      <span className="ext-chip" data-away={away || undefined}><Icon name="machine" className="size-3" />{machine}</span>
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
  // Research is never merged: ready, it is closed.
  const tag = <span className="tag font-normal tracking-normal" data-progress={p}>{t(p === 'ready' && unit.kind === 'research' ? 'unit.progress.readyClose' : `unit.progress.${p}`)}</span>
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

/** Longer than this, a name in a chip is shortened in the middle. */
const CHIP_CHARS = 30

/**
 * A name or path inside a chip: on one line, shortened in the middle when long so its start
 * and its extension stay ("2026-11-16-new-acc…source.md"), and cut at the end by the chip's
 * width as a last resort. The whole name is in the tooltip and read out by screen readers.
 */
export function ChipName({ name }: { name: string }) {
  if (name.length <= CHIP_CHARS) return <span className="chip-name">{name}</span>
  const ext = /\.[A-Za-z0-9]{1,8}$/.exec(name)?.[0] ?? ''
  const tail = name.slice(-Math.max(ext.length + 6, 10))
  const shown = `${name.slice(0, CHIP_CHARS - tail.length - 1)}…${tail}`
  return (
    <Tip text={name}>
      <span className="chip-name">
        <span aria-hidden>{shown}</span>
        <span className="sr-only">{name}</span>
      </span>
    </Tip>
  )
}

/** Web links in plain text, cut before trailing punctuation. */
const LINK = /https?:\/\/[^\s<>"'）。，、]+[^\s<>"'）。，、.,;:!?)]/g

/** Quiet text on the dark rail or on the page. */
const QUIET = { rail: 'text-rail-ink-2 [&_:is(a,button):hover]:text-rail-ink', page: 'text-ink-2 [&_:is(a,button):hover]:text-ink' }

/**
 * An intent's note: plain text with its line breaks, its links clickable. A long one shows
 * its first lines, with "show more".
 */
export function IntentNote({ text, tone }: { text: string; tone: 'rail' | 'page' }) {
  const { t } = useT()
  const box = useRef<HTMLParagraphElement>(null)
  const [long, setLong] = useState(false)
  const [all, setAll] = useState(false)
  // Measured while clamped: more than fits in the first lines.
  useLayoutEffect(() => {
    const el = box.current
    if (el && !all) setLong(el.scrollHeight > el.clientHeight + 1)
  }, [text, all])
  const parts: ReactNode[] = []
  let last = 0
  for (const m of text.matchAll(LINK)) {
    parts.push(text.slice(last, m.index))
    parts.push(<a key={m.index} href={m[0]} target="_blank" rel="noreferrer" className="underline underline-offset-2">{m[0]}</a>)
    last = m.index + m[0].length
  }
  parts.push(text.slice(last))
  return (
    <div className={cn('text-xs', QUIET[tone])}>
      <p ref={box} className={cn('whitespace-pre-line wrap-anywhere', !all && 'line-clamp-3')}>{parts}</p>
      {(long || all) && (
        <button type="button" aria-expanded={all} className="mt-0.5 rounded-sm text-[11px] font-semibold" onClick={() => setAll(a => !a)}>
          {t(all ? 'rail.noteLess' : 'rail.noteMore')}
        </button>
      )}
    </div>
  )
}

const MarkdownView = lazy(() => import('./Markdown'))

/**
 * Longer text an agent or the user wrote (a task's input, a delivery note, a gate's note, an
 * answer): short Markdown, rendered compact by the safe renderer, which loads the first time
 * one shows; until then the plain text with its line breaks. With `fold`, a long one shows its
 * first lines, with "show more".
 */
export function Prose({ text, fold = true, onFile }: { text: string; fold?: boolean; onFile?: (path: string) => void }) {
  const { t } = useT()
  const outer = useRef<HTMLDivElement>(null)
  const inner = useRef<HTMLDivElement>(null)
  const [long, setLong] = useState(false)
  // Opened for this text: another task's text starts cut again.
  const [opened, setOpened] = useState<string | null>(null)
  const all = opened === text
  // Measured while cut, and again when the rendered Markdown replaces the plain text.
  useEffect(() => {
    const o = outer.current, i = inner.current
    if (!o || !i) return
    const check = () => setLong(i.offsetHeight > o.clientHeight + 1)
    const watch = new ResizeObserver(check)
    watch.observe(i)
    watch.observe(o)
    return () => watch.disconnect()
  }, [fold])
  const body = <Suspense fallback={<div className="whitespace-pre-line">{text}</div>}><MarkdownView text={text} compact onFile={onFile} /></Suspense>
  if (!fold) return body
  return (
    <div>
      <div ref={outer} className="prose-fold" data-cut={(!all && long) || undefined} data-all={all || undefined}><div ref={inner}>{body}</div></div>
      {(long || all) && (
        <button type="button" aria-expanded={all} className="mt-0.5 rounded-sm text-[11px] font-semibold text-ink-2 hover:text-ink" onClick={() => setOpened(all ? null : text)}>
          {t(all ? 'rail.noteLess' : 'rail.noteMore')}
        </button>
      )}
    </div>
  )
}

/** A note behind a small disclosure, where rows must stay compact (the roadmap). */
export function NoteDisclosure({ text, tone }: { text: string; tone: 'rail' | 'page' }) {
  const { t } = useT()
  return (
    <Collapsible className={QUIET[tone]}>
      <CollapsibleTrigger className="group/note mt-0.5 flex items-center gap-0.5 rounded-sm text-[11px] font-semibold">
        <Icon name="chev" className="size-3 transition-transform group-data-[state=open]/note:rotate-90" />{t('rail.note')}
      </CollapsibleTrigger>
      <CollapsibleContent className="pt-0.5"><IntentNote text={text} tone={tone} /></CollapsibleContent>
    </Collapsible>
  )
}
