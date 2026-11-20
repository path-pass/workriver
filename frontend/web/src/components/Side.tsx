// The side panel, right of the main area: what asks something of the user (the inbox),
// where the agents are, what was committed lately, the backlog, the machines' CPU and memory,
// or asking why about the picked task – one at a time. Neither is looked at all the time, so the panel folds
// to a thin strip; on a narrow screen the same content opens as a sheet over the page.
import { useEffect, useMemo, useRef, type ReactNode } from 'react'
import { Button } from '@/ui/primitives/button'
import { ScrollArea } from '@/ui/primitives/scroll-area'
import { Sheet, SheetContent, SheetTitle } from '@/ui/primitives/sheet'
import { ms } from '@/planning/select'
import { commitGraph, commitStats, laneTone, type GraphRow } from '@/code/select'
import { baseName } from '@/code/files'
import type { Pick } from '@/work/types'
import { contentLang, useT } from '@/i18n'
import { useResize } from '@/lib/useResize'
import type { Workspace } from '@/workspace/types'
import { BacklogList } from './Backlog'
import { InboxList } from './Inbox'
import { MachinesList } from '@/machines/Machines'
import { RecentRuns, RunBox } from '@/commands/Run'
import { Who } from '@/agents/Mark'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { Agents } from '@/agents/Agents'

export type Section = 'inbox' | 'agents' | 'commits' | 'backlog' | 'machines' | 'run' | 'ask'
export const SECTIONS: Section[] = ['inbox', 'agents', 'commits', 'backlog', 'machines', 'run', 'ask']

/** A commit opened from the panel: the code block shows it. `at` changes with every request. */
export type CommitFocus = { unit: string; id: string; at: number }

interface ContentProps {
  ws: Workspace
  /** The unit and pick on screen: the agent line that leads there shows as picked. */
  here: { unit: string; pick: Pick } | null
  onOpen: (unit: string, pick: Pick) => void
  section: Section
  onCommit: (unit: string, task: string, commit: string) => void
  commit: CommitFocus | null
  /** The project picked in the rail, or null for all: the backlog shows its findings. */
  project: string | null
  /** The Ask section, made by the caller: it lays itself out (its box stays at the bottom). */
  ask: ReactNode
  /** Shows another section (a machine's alert in Needs you opens Machines). */
  onSection: (s: Section) => void
  /** Opens Ask on a workspace's conversation with its manager (unread messages in Needs you). */
  onTalk: (workspace: string) => void
}

/** Lane spacing, the base's x, and a row's height in the commit graph (px; integers keep 2 px lines crisp). */
const LANE = 12, X0 = 8, ROW = 44

/** One row of the graph: the base, every lane passing through, the row's own dot. */
function GraphCell({ row, lanes }: { row: GraphRow; lanes: number }) {
  const x = (k: number) => X0 + k * LANE
  const mid = ROW / 2, b = x(0)
  return (
    <svg aria-hidden className="graph" width={x(lanes) + X0} height={ROW}>
      <line className="base" x1={b} y1={0} x2={b} y2={ROW} />
      {row.segs.map(g => {
        const c = x(g.lane)
        return (
          <g key={g.unit.id} style={{ color: `var(--lane-${laneTone(g.unit.id)})` }}>
            {g.up === 'line' && <line x1={c} y1={0} x2={c} y2={mid} data-dash={g.upDash || undefined} />}
            {g.up === 'join' && <path d={`M${c} ${mid}C${c} ${mid / 2} ${b} ${mid / 2} ${b} 0`} data-dash={g.upDash || undefined} />}
            {g.down === 'line' && <line x1={c} y1={mid} x2={c} y2={ROW} data-dash={g.downDash || undefined} />}
            {g.down === 'fork' && <path d={`M${c} ${mid}C${c} ${mid * 1.5} ${b} ${mid * 1.5} ${b} ${ROW}`} data-dash={g.downDash || undefined} />}
          </g>
        )
      })}
      <circle className="dot" data-live={row.writing ? true : undefined} style={{ color: `var(--lane-${laneTone(row.unit.id)})` }} cx={x(row.lane)} cy={mid} r={4} />
    </svg>
  )
}

/** A line of the graph without a commit: the base runs through it (from its middle, at the top), with what it holds beside. */
function BaseLine({ lanes, top = false, children }: { lanes: number; top?: boolean; children: ReactNode }) {
  return (
    <div className="flex h-6 items-center gap-1.5 pl-0.5">
      <svg aria-hidden className="graph" width={X0 + lanes * LANE + X0} height={24}><line className="base" x1={X0} y1={top ? 12 : 0} x2={X0} y2={24} /></svg>
      {children}
    </div>
  )
}

/**
 * Every unit's commits as a git graph, newest first: the base on the left, a lane per unit
 * branch in its colour, leaving the base at its first commit and, merged, joining it again.
 * On top, dashed, the work agents are writing right now. Only the project picked in the rail,
 * when one is.
 */
function Commits({ ws, here, onCommit, commit: focus, project }: Omit<ContentProps, 'section' | 'onOpen' | 'ask' | 'onSection' | 'onTalk'>) {
  const { t, num, waited, date } = useT()
  const graphs = useMemo(() => commitGraph(project ? { ...ws, intents: ws.intents.filter(i => i.project === project) } : ws), [ws, project])
  if (!graphs.length) return <p className="px-2.5 text-sm text-ink-2">{t('commits.none')}</p>
  return (
    <div className="flex flex-col gap-4">
      {graphs.map(({ repo, base, rows, lanes }) => (
        <section key={repo?.path ?? ''} aria-label={graphs.length > 1 ? repo?.name : undefined}>
          {graphs.length > 1 && <h3 className="mb-1 px-2.5 font-mono text-xs font-semibold text-ink-2">{repo?.name}</h3>}
          {/* The base lane, labelled with its branch. */}
          <BaseLine lanes={lanes} top><span className="min-w-0 truncate font-mono text-[11px] text-ink-2">{base}</span></BaseLine>
          {rows[0]?.writing && <BaseLine lanes={lanes}><h3 className="text-xs font-semibold text-amber-ink">{t('commits.writing')}</h3></BaseLine>}
          <div role="list">
            {rows.map(row => {
              const { unit, commit, task, writing } = row
              const cell = <GraphCell row={row} lanes={lanes} />
              if (writing)
                return (
                  <div key={unit.id + writing.node.id} role="listitem" className="grow" data-live>
                    {cell}
                    <span className="min-w-0">
                      <span className="block truncate font-mono text-xs">{writing.files.map(baseName).join(' ')}</span>
                      <span className="block truncate text-xs text-ink-2" lang={contentLang(unit.title)}>{unit.title}{writing.node.taskId && ` #${writing.node.taskId}`}</span>
                    </span>
                    {/* In the last column, where a commit's age is: its ring never reaches the lanes. */}
                    <span className="flex-none px-1"><Who name={writing.node.agent ?? '?'} live /></span>
                  </div>
                )
              const c = commit!, s = commitStats(c)
              return (
                <div key={unit.id + c.id} role="listitem">
                  <button
                    type="button"
                    className="grow"
                    disabled={!task}
                    aria-pressed={focus?.id === c.id && here?.unit === unit.id}
                    onClick={() => task && onCommit(unit.id, task.id, c.id)}
                  >
                    {cell}
                    <span className="min-w-0">
                      <span className="block truncate" lang={contentLang(c.subject)}>{c.subject}</span>
                      <span className="flex items-center gap-1.5 text-xs text-ink-2">
                        <span className="truncate" lang={contentLang(unit.title)}>{unit.title}</span>
                        {task?.state === 'unregistered'
                          ? <span className="flex flex-none items-center gap-0.5 text-del"><Icon name="no" className="size-3" />{t('need.loose')}</span>
                          : task?.taskId && <span className="flex-none font-mono">#{task.taskId}</span>}
                        <span className="flex-none font-mono"><span className="text-add">+{num(s.add)}</span> <span className="text-del">−{num(s.del)}</span></span>
                      </span>
                    </span>
                    <Tip text={date(c.date)}><span className="flex-none self-start pt-[5px] text-xs text-ink-2">{waited(ms(c.date))}</span></Tip>
                  </button>
                </div>
              )
            })}
          </div>
        </section>
      ))}
    </div>
  )
}

function Content({ ws, here, onOpen, section, onCommit, commit, project, onSection, onTalk }: Omit<ContentProps, 'ask'>) {
  const { t, num } = useT()
  const live = ws.sessions.filter(s => !s.ended).length
  return (
    <div className="sidepane flex flex-col px-2 pb-6">
      {section === 'inbox' ? (
        <section aria-labelledby="side-inbox">
          <h2 id="side-inbox" className="mb-2 px-2.5 text-sm font-[650]">{t('inbox.title')}</h2>
          <InboxList ws={ws} onOpen={onOpen} onMachines={() => onSection('machines')} onTalk={onTalk} />
        </section>
      ) : section === 'machines' ? (
        <section aria-labelledby="side-machines">
          <h2 id="side-machines" className="mb-2 px-2.5 text-sm font-[650]">{t('metrics.title')}</h2>
          <MachinesList />
        </section>
      ) : section === 'backlog' ? (
        <section aria-labelledby="side-backlog">
          <h2 id="side-backlog" className="mb-2 px-2.5 text-sm font-[650]">{t('backlog.title')}</h2>
          <BacklogList ws={ws} project={project} onOpen={onOpen} />
        </section>
      ) : section === 'run' ? (
        <section aria-labelledby="side-run">
          <h2 id="side-run" className="mb-2 px-2.5 text-sm font-[650]">{t('run.section')}</h2>
          <RunBox ws={ws} unit={here?.unit ?? null} project={project} />
          <RecentRuns runs={ws.runs ?? []} />
        </section>
      ) : section === 'commits' ? (
        <section aria-labelledby="side-commits">
          <h2 id="side-commits" className="mb-2 px-2.5 text-sm font-[650]">{t('commits.title')}</h2>
          <Commits ws={ws} here={here} onCommit={onCommit} commit={commit} project={project} />
        </section>
      ) : (
        <section aria-labelledby="side-agents">
          <h2 id="side-agents" className="mb-2 flex items-baseline gap-1.5 px-2.5 text-sm font-[650]">
            {t('agents.title')}<span className="text-xs font-normal text-ink-2">{num(live)}</span>
          </h2>
          <Agents ws={ws} here={here} onOpen={onOpen} />
        </section>
      )}
    </div>
  )
}

/** Ask keeps its box at the bottom, so it scrolls inside itself; the others scroll whole. */
function Body(props: ContentProps) {
  if (props.section === 'ask') return props.ask
  return <ScrollArea className="min-h-0 flex-1 [&>[data-slot=scroll-area-viewport]>div]:block!"><Content {...props} /></ScrollArea>
}

/** The buttons work as a switch: one section at a time; the open one again folds it. */
export function SideButtons({ active, onPick }: { active: Section | null; onPick: (s: Section) => void }) {
  const { t } = useT()
  const button = (s: Section, icon: string, label: string) => (
    <Tip text={label}>
      <Button variant="ghost" size="icon-sm" className="aria-pressed:bg-you-bg aria-pressed:text-you-ink" aria-label={label} aria-pressed={active === s} onClick={() => onPick(s)}>
        <Icon name={icon} />
      </Button>
    </Tip>
  )
  return <>{button('inbox', 'inbox', t('inbox.title'))}{button('agents', 'agents', t('agents.title'))}{button('commits', 'commits', t('commits.title'))}{button('backlog', 'backlog', t('backlog.title'))}{button('machines', 'machine', t('metrics.title'))}{button('run', 'terminal', t('run.section'))}{button('ask', 'talk', t('ask.why'))}</>
}

interface PanelProps extends Omit<ContentProps, 'section'> {
  /** The section shown; null when folded. */
  section: Section | null
  onClose: () => void
  /** Its width once a drag settles, so a graph beside it can refit. */
  onWidth?: (w: number) => void
}

/** Wide screens: a column of its own that folds to a strip of two buttons. */
export function SidePanel({ section, onSection, onClose, onWidth, ...content }: PanelProps) {
  const { t } = useT()
  // The left edge drags the width; double-click: the default.
  const size = useResize({ key: 'workriver.side.width', variable: '--side-w', def: 340, min: 280, max: 720, edge: 'left' })
  useEffect(() => onWidth?.(size.width), [onWidth, size.width])
  if (!section)
    return (
      <aside className="sticky top-0 flex h-screen w-11 flex-col items-center gap-1 border-l border-line bg-surface pt-3 max-[860px]:hidden">
        <SideButtons active={null} onPick={onSection} />
      </aside>
    )
  return (
    <aside className="sticky top-0 flex h-screen w-[var(--side-w,340px)] flex-col border-l border-line bg-surface max-[860px]:hidden">
      <div {...size.handle} aria-label={t('side.resize')} className="edge" data-edge="left" />
      <div className="flex items-center gap-1 px-2 pt-3 pb-2">
        <SideButtons active={section} onPick={onSection} />
        <Tip text={t('side.close')}>
          <Button variant="ghost" size="icon-sm" className="ml-auto" aria-label={t('side.close')} onClick={onClose}><Icon name="fold" /></Button>
        </Tip>
      </div>
      <Body section={section} onSection={onSection} {...content} />
    </aside>
  )
}

/** Narrow screens: the same content as a sheet over the page, with the same switch. */
export function SideSheet({ section, onSection, onClose, ...content }: PanelProps) {
  const { t } = useT()
  // The sheet has no trigger of its own (several buttons open it): focus goes back to the one used.
  const opener = useRef<HTMLElement | null>(null)
  return (
    <Sheet open={!!section} onOpenChange={o => { if (!o) onClose() }}>
      <SheetContent
        aria-describedby={undefined}
        onOpenAutoFocus={() => { opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null }}
        onCloseAutoFocus={e => { if (opener.current?.isConnected) { e.preventDefault(); opener.current.focus() } }}
      >
        <div className="flex items-center gap-1 px-2 pt-3 pb-2">
          <SheetTitle className="sr-only">{t('side.title')}</SheetTitle>
          <SideButtons active={section} onPick={onSection} />
          <Button variant="ghost" size="icon-sm" className="ml-auto" aria-label={t('side.close')} onClick={onClose}><Icon name="no" /></Button>
        </div>
        {section && <Body section={section} onSection={onSection} {...content} />}
      </SheetContent>
    </Sheet>
  )
}
