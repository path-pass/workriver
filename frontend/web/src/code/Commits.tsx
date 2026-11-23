// The side panel's Commits section: every unit's commits, newest first, as a git graph.
import { useMemo, type ReactNode } from 'react'
import { ms } from '@/planning/select'
import { commitGraph, commitStats, laneTone, type GraphRow } from './select'
import { baseName } from './files'
import { contentLang, useT } from '@/i18n'
import { Who } from '@/agents/Mark'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import type { CommitFocus } from './types'
import type { Pick } from '@/work/types'
import type { Workspace } from '@/workspace/types'

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
export function Commits({ ws, here, onCommit, commit: focus, project }: { ws: Workspace; here: { unit: string; pick: Pick } | null; onCommit: (unit: string, task: string, commit: string) => void; commit: CommitFocus | null; project: string | null }) {
  const { t, num, waited, date } = useT()
  const graphs = useMemo(() => commitGraph(project ? { ...ws, intents: ws.intents.filter(i => i.project === project) } : ws), [ws, project])
  if (ws.gitPending) return null
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
