// Small parts used by every layer: the icon set, a tooltip, and an agent's mark.
import { ArrowLeft, ArrowUp, Bot, Crown, FlaskConicalOff, GitCommitHorizontal, Check, CircleDashed, ChevronRight, Circle, Clock, Copy, CornerUpRight, Eye, FlaskConical, Hand, ImagePlus, Inbox, ListTodo, MessageSquare, Moon, PanelLeftClose, PanelLeftOpen, PanelRightClose, Plus, SquareTerminal, Users, Pencil, Rocket, Shield, X, type LucideIcon } from 'lucide-react'
import { useEffect, useState, type ReactElement, type ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { contentLang, useT } from '@/i18n'
import { cn } from '@/lib/utils'
import { focusAgent, NOTE } from '@/runtime'
import type { Step } from '@/types'

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
  chore: ListTodo,
  inbox: Inbox,
  agents: Users,
  fold: PanelRightClose,
  idle: Moon,
  copy: Copy,
  terminal: SquareTerminal,
  talk: MessageSquare,
  send: ArrowUp,
  attach: ImagePlus,
  plus: Plus,
  untested: FlaskConicalOff,
  commits: GitCommitHorizontal,
  rail: PanelLeftClose,
  'rail-open': PanelLeftOpen,
  no: X,
  'security-review': Shield,
  'code-review': Eye,
  test: FlaskConical,
  far: CornerUpRight,
  chev: ChevronRight,
  back: ArrowLeft,
}

export function Icon({ name, className }: { name: string; className?: string }) {
  const Glyph = ICONS[name] ?? Circle
  return <Glyph aria-hidden size={16} strokeWidth={1.5} className={cn('ic', className)} />
}

/** Content (a title, what someone typed) in its own element, marked with its guessed language. */
export const Content = ({ text }: { text: string }) => <span lang={contentLang(text)}>{text}</span>

/**
 * A UI sentence with content in it: translate with SLOT as the content's value, and the
 * content goes where SLOT landed, so it keeps its own language inside the UI's.
 */
export const SLOT = ''
export function Slot({ text, children }: { text: string; children: ReactNode }) {
  const [before, after = ''] = text.split(SLOT)
  return <>{before}{children}{after}</>
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

/** Copies `text`; with a `label` it is a small text button, without, an icon for a command. */
export function CopyButton({ text, label }: { text: string; label?: string }) {
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
  return (
    <Button variant="ghost" size="icon-xs" className="flex-none" aria-label={t(copied ? 'steps.copied' : 'steps.copy')} onClick={copy}>
      <Icon name={copied ? 'check' : 'copy'} className="size-3.5" />
    </Button>
  )
}

/** What an agent prepared for the user to do by hand: the same for a gate and for a chore. */
export function Steps({ steps }: { steps: Step[] }) {
  return (
    <ol className="steps">
      {steps.map((s, k) => (
        <li key={k}>
          <span lang={contentLang(s.text)}>{s.text}</span>
          {s.command && <div className="cmd"><code>{s.command}</code><CopyButton text={s.command} /></div>}
        </li>
      ))}
    </ol>
  )
}

// Names one letter apart land far apart on the colour wheel.
const hue = (name: string) => [...name].reduce((n, ch) => (n * 31 + ch.charCodeAt(0) * 97) % 360, 7)

/** An agent as a small coloured mark, the same wherever it shows up. */
export function Who({ name, live = false }: { name: string; live?: boolean }) {
  const { t } = useT()
  const initials = name.split(/[-_\s]+/).filter(Boolean).map(w => w[0]).join('').slice(0, 2).toUpperCase()
  return (
    <Tip text={live ? t('who.writing', { name }) : name}>
      <span className="who" data-live={live || undefined} style={{ background: `hsl(${hue(name)} 55% 40%)` }}>
        {initials}
      </span>
    </Tip>
  )
}

/** Opens the agent's terminal. A click target of its own, so it never also picks a row. */
export function TerminalButton({ agent, className }: { agent: string; className?: string }) {
  const { t } = useT()
  return (
    <Tip text={t('terminal.open')}>
      <Button
        variant="ghost"
        size="icon-xs"
        className={cn('flex-none', className)}
        aria-label={t('terminal.openFor', { agent })}
        onClick={e => { e.stopPropagation(); focusAgent(agent) }}
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
