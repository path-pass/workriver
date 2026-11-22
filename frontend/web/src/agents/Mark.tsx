// An agent as a small coloured mark, and the button that opens its terminal view.
import { createContext, useContext } from 'react'
import { Button } from '@/ui/primitives/button'
import { useT } from '@/i18n'
import { cn } from '@/lib/utils'
import { AgentContext } from './context'
import { Tip } from '@/ui/Tip'
import { Icon } from '@/ui/Icon'

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

/** Opens the agent's terminal view on the page. A click target of its own, so it never also picks a row. */
export function TerminalButton({ agent, session, className }: { agent: string; session?: string | null; className?: string }) {
  const { t } = useT()
  const { view } = useContext(AgentContext)
  return (
    <Tip text={t('terminal.open')}>
      <Button
        variant="ghost"
        size="icon-xs"
        className={cn('flex-none', className)}
        aria-label={t('terminal.openFor', { agent })}
        onClick={e => { e.stopPropagation(); view({ name: agent, session }) }}
      >
        <Icon name="terminal" className="size-3.5" />
      </Button>
    </Tip>
  )
}
