// The side panel's Agents section: the agents as a tree, by the workspace they run in.
import { useContext } from 'react'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/ui/primitives/collapsible'
import { agentTree, type TreeAgent } from './select'
import { contentLang, useT } from '@/i18n'
import { NodeName } from '@/work/Nodes'
import { TerminalButton, Who } from './Mark'
import { AgentContext } from './context'
import { MachineChip } from '@/machines/format'
import { Icon, Mark } from '@/ui/Icon'
import type { Pick } from '@/work/types'
import type { Workspace } from '@/workspace/types'

/** One agent of the tree: the line opens its work (and, blocked, its gate), the icon at its
 *  end its terminal view; under it, its question agents. */
function AgentLine({ a, here, onOpen }: { a: TreeAgent; here: { unit: string; pick: Pick } | null; onOpen: (unit: string, pick: Pick) => void }) {
  const { t, waited } = useT()
  const agents = useContext(AgentContext)
  const { s, state, at } = a
  const ago = Number.isFinite(a.since) ? waited(a.since) : ''
  const word = a.closable
    ? <span className="closable"><Icon name="idle" className="size-3" />{ago ? t('agents.closableFor', { time: ago }) : t('agents.closable')}</span>
    : state === 'idle' ? (ago ? t('agents.idleFor', { time: ago }) : t('agents.state.idle'))
    : state === 'blocked' && ago ? t('agents.blockedFor', { time: ago }) : t(`agents.state.${state}`)
  // What it is on (or held, or last did); a question agent, who asked.
  const sub = [
    s.harness && <span key="h" className="flex-none font-mono">{s.harness}</span>,
    a.asker !== undefined && <span key="q" className="flex-none">{a.asker ? t('agents.askedBy', { who: a.asker }) : t('agents.askedByYou')}</span>,
    at ? (
      <span key="t" className="task min-w-0 truncate">
        {!a.current && <span className="mr-1">{t('agents.last')}</span>}
        <NodeName n={at.node} /> · <span lang={contentLang(at.unit.title)}>{at.unit.title}</span>
      </span>
    ) : s.topic && <span key="p" className="min-w-0 truncate" lang={contentLang(s.topic)}>{s.topic}</span>,
  ].filter(Boolean)
  return (
    <li>
      <div className="agent-row" data-away={s.unreachable || undefined}>
        <button
          type="button"
          className="agent"
          data-state={state}
          disabled={!at && state !== 'blocked'}
          aria-pressed={!!at && here?.unit === at.unit.id && here.pick?.id === at.node.id}
          onClick={() => {
            if (at) onOpen(at.unit.id, { kind: 'task', id: at.node.id })
            if (state === 'blocked') agents.answer({ name: s.agent, session: s.session })
          }}
        >
          <Who name={s.agent} topic={s.topic} live={state === 'working' && !!at?.node.writing} />
          <span className="agent-name">
            <span className="min-w-0 truncate font-[550]">{s.agent}</span>
            {s.manager && <Mark name="manager" label={t('manager.mark')} />}
            {/* Waiting for a person in its terminal: the Blocking meaning, in violet. */}
            {state === 'blocked' && <span className="text-you-ink"><Mark name="blocked" label={t('agents.blocked')} /></span>}
            <MachineChip machine={s.machine} away={s.unreachable} />
          </span>
          <span className="word">{word}</span>
          {/* Under the name and the word: it takes the whole width left of the terminal button. */}
          {sub.length > 0 && <span className="sub">{sub.flatMap((x, k) => (k ? [<span key={`d${k}`} aria-hidden className="flex-none">·</span>, x] : [x]))}</span>}
        </button>
        {state !== 'gone' && <TerminalButton agent={s.agent} session={s.session} />}
      </div>
      {a.kids.length > 0 && <ul className="tree">{a.kids.map(k => <AgentLine key={k.s.session ?? k.s.agent} a={k} here={here} onOpen={onOpen} />)}</ul>}
    </li>
  )
}

/**
 * The agents as a tree, by the runtime workspace they run in: the workspace and its manager,
 * the workers under it, the question agents under the worker whose task they ask about. Last,
 * folded, the sessions that never took a task.
 */
export function Agents({ ws, here, onOpen }: { ws: Workspace; here: { unit: string; pick: Pick } | null; onOpen: (unit: string, pick: Pick) => void }) {
  const { t, num } = useT()
  const { groups, own } = agentTree(ws)
  const line = (a: TreeAgent) => <AgentLine key={a.s.session ?? a.s.agent} a={a} here={here} onOpen={onOpen} />
  // The project a workspace works on, on its label's hover.
  const worksOn = (id: string) => { const w = ws.workspaces?.find(x => x.id === id); return w?.projectTitle ? t('intent.worksOn', { project: w.projectTitle }) : undefined }
  return (
    <div className="agents flex flex-col gap-2">
      {groups.map(g => (
        <section key={g.key} aria-label={g.label || undefined} className="flex flex-col">
          {g.label && (
            <h3 className="flex items-center gap-1.5 px-3.5 pt-2 pb-0.5 text-xs font-semibold text-ink-2">
              {/* The workspace's label keeps its room; the intent's title gives way. */}
              <span className="max-w-1/2 flex-none truncate" title={worksOn(g.key)} lang={contentLang(g.label)}>{g.label}</span>
              {/* A workspace of another machine, not linked to one here: its own group. */}
              <MachineChip machine={g.machine} />
              {/* Where the workspace's new work goes: its current intent. */}
              {g.intent && g.intent.title !== g.label && <span className="min-w-0 truncate font-normal">· <span lang={contentLang(g.intent.title)}>{g.intent.title}</span></span>}
              {!g.manager && <span className="flex flex-none items-center gap-1 font-normal opacity-70"><Icon name="manager" className="size-3" />{t('manager.state.none')}</span>}
            </h3>
          )}
          <ul className="tree" data-root>
            {g.manager ? <AgentLine a={{ ...g.manager, kids: g.workers }} here={here} onOpen={onOpen} /> : g.workers.map(line)}
          </ul>
        </section>
      ))}
      {own.length > 0 && (
        <Collapsible>
          <CollapsibleTrigger className="group/own flex w-full items-center gap-1 rounded-sm px-3.5 pt-2 pb-0.5 text-xs font-semibold text-ink-2 hover:text-ink">
            <Icon name="chev" className="size-3.5 transition-transform group-data-[state=open]/own:rotate-90" />
            {t('agents.own')}<span className="font-normal">{num(own.length)}</span>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <p className="px-3.5 pb-1 text-[11px] text-ink-2">{t('agents.ownHint')}</p>
            <ul className="tree" data-root>{own.map(line)}</ul>
          </CollapsibleContent>
        </Collapsible>
      )}
    </div>
  )
}
