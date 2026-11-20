// The graph's nodes and lines: a task as a card, the start as a dot, a hand-over as a line.
import { getBezierPath, Handle, Position, useNodes, type Edge, type EdgeProps, type Node, type NodeProps } from '@xyflow/react'
import { untested, finderLabel, findingsOf, nodeLabel, openCount, reviewLabel, severityCounts } from './select'
import { useT, type T } from '@/i18n'
import { type Growth } from './growth'
import type { Task } from './types'
import { Who } from '@/agents/Mark'
import { Content, Slot, SLOT } from '@/content/Chips'
import { Icon, Mark, Waiting } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { W, H } from './layout'

// ---- node and line types ----

export type CardData = { node: Task; dim: boolean; picked: boolean; onPick: (id: string) => void; talks: number; grew?: Growth; held?: boolean }
export type DotData = { title: string }
export type LineData = { state?: 'flowed' | 'flowing'; also: boolean; off: boolean }

export const glyph = (n: Task) =>
  n.kind === 'review' ? n.reviewer ?? 'ring'
  : n.kind === 'gate' ? `gate-${n.gate}`
  : n.design && (n.state === 'done' || n.state === 'todo') ? 'design'
  : n.research && (n.state === 'done' || n.state === 'todo') ? 'research'
  : ({ done: 'check', running: 'pen', unregistered: 'no', todo: 'ring', waiting: 'ring', dropped: 'ring' })[n.state]

const ports = (
  <>
    <Handle type="target" position={Position.Left} isConnectable={false} />
    <Handle type="source" position={Position.Right} isConnectable={false} />
  </>
)

/** A task's name: a review's label is UI text; a task's title is content, in its own language. */
export function NodeName({ n }: { n: Task }) {
  const { t } = useT()
  if (n.kind === 'review') return reviewLabel(t, n)
  const title = <Content text={n.title} />
  if (n.state === 'unregistered') return <Slot text={t('task.unregistered', { title: SLOT })}>{title}</Slot>
  return <>{n.taskId && `#${n.taskId} `}{title}</>
}

/** A review's label: the reviewer's name with the punctuation right after it (「：」 may not
 *  start a line) on one line, then its state. A name too long for the node is what gets cut
 *  (an ellipsis); the punctuation and the state keep their place. */
function ReviewLabel({ n }: { n: Task }) {
  const { t } = useT()
  const by = finderLabel(t, n), label = reviewLabel(t, n), i = label.indexOf(by)
  if (i < 0) return label
  const end = i + by.length + label.slice(i + by.length).match(/^\p{P}*/u)![0].length
  return <>{label.slice(0, i)}<span className="head"><span className="nm">{by}</span><span>{label.slice(i + by.length, end)}</span></span>{label.slice(end)}</>
}
/** Where the scope grew at a task: which folders, beyond which work before it. */
export const grewText = (t: T, g: Growth) => t('flow.grew', {
  folders: g.folders.map(f => f || './').join(t('list.sep')),
  tasks: g.beyond.map(x => (x.taskId ? `#${x.taskId}` : nodeLabel(t, x))).join(t('list.sep')),
})

// What kind of work is a shape (set in CSS from data-kind): square corners for
// implementing, a pill for a review, a double frame for a gate. A review says how much it found per severity and
// carries a violet count while findings of it wait for the user.
function WorkCard({ data }: NodeProps<Node<CardData>>) {
  const { node: n, dim, picked, onPick, talks, grew, held } = data
  const { t, num } = useT()
  const open = openCount(n)
  return (
    <>
      <Tip text={<span><NodeName n={n} /></span>}>
      <button
        type="button"
        className="wnode"
        data-kind={n.kind}
        data-state={n.kind === 'review' ? undefined : n.state}
        data-dim={dim || undefined}
        data-dropped={n.state === 'dropped' || undefined}
        aria-pressed={picked}
        // The mouse picks through React Flow's node click, which a drag does not trigger;
        // this handler is for the keyboard (detail 0).
        onClick={e => { if (e.detail === 0) onPick(n.id) }}
      >
        <Icon
          name={glyph(n)}
          className={n.kind === 'review' && n.state !== 'dropped' ? (n.result === 'running' ? 'text-amber!' : 'text-water!') : ''}
        />
        <span className="lb">
          {n.kind === 'review'
            ? n.result === 'found' ? <><span className="nm">{finderLabel(t, n)}</span><span className="sev">{severityCounts(findingsOf(n))}</span></> : <ReviewLabel n={n} />
            : n.state === 'unregistered' || !n.taskId ? <NodeName n={n} /> : <><b>#{n.taskId}</b> <Content text={n.title} /></>}
        </span>
        {n.agent && (n.state === 'running' || n.state === 'todo') && <Who name={n.agent} live={n.writing} />}
        {open > 0 && <Waiting count={open} />}
        {/* Questions asked about this work: information, not a button. */}
        {/* Work handed in with no check run: information, not a call to act. */}
        {untested(n) && <span className="untested"><Mark name="untested" label={t('evidence.none')} /></span>}
        {n.blind && <span className="blind"><Mark name="blind" label={t('task.blindHint')} /></span>}
        {n.state === 'dropped' && <span className="dropped"><Mark name="dropped" label={t('task.dropped')} /></span>}
        {/* A blocker gate (top left; gates are never blind), and the work it holds up while it is
            open (bottom left; only work not done or dropped, so never with untested or dropped). */}
        {n.blocks && <span className="blocker"><Mark name="blocker" label={t('flow.blocker')} /></span>}
        {held && <span className="held"><Mark name="blocked" label={t('flow.held')} /></span>}
        {grew && (
          <Tip text={grewText(t, grew)}>
            <span className="grew"><Icon name="grew" /><span aria-hidden>+{num(grew.folders.length)}</span><span className="sr-only">{grewText(t, grew)}</span></span>
          </Tip>
        )}
        {talks > 0 && (
          <span className="talks"><Icon name="talk" className="size-2.5" /><span aria-hidden>{num(talks)}</span><span className="sr-only">{t('ask.count', { count: talks })}</span></span>
        )}
      </button>
      </Tip>
      {ports}
    </>
  )
}
function Dot({ data }: NodeProps<Node<DotData>>) {
  return (
    <>
      <Tip text={<Content text={data.title} />}><div className="wdot" /></Tip>
      {ports}
    </>
  )
}
// A line between neighbouring columns is a curve. A line that would cross other columns
// runs through the empty space instead: out to the right at its own height, along the gap
// between two rows where that height meets a node, and down the gap before its target.
const GAP = 56, ROW = 24
type Box = { x: number; y: number; w: number; h: number }
type Point = [number, number]

function route([sx, sy]: Point, [tx, ty]: Point, boxes: Box[]): Point[] {
  const cx = tx - GAP / 2, gx = sx + GAP / 2
  const hits = (y: number) => boxes.some(b => b.x < cx && b.x + b.w > gx && b.y - 4 < y && b.y + b.h + 4 > y)
  const cuts = (y: number) => boxes.some(b => b.x < gx && b.x + b.w > gx && b.y < Math.max(sy, y) && b.y + b.h > Math.min(sy, y))
  const straight: Point[] = [[sx, sy], [cx, sy], [cx, ty], [tx, ty]]
  if (!hits(sy)) return straight
  const fy = boxes
    .flatMap(b => [b.y - ROW / 2, b.y + b.h + ROW / 2])
    .sort((a, b) => Math.abs(a - sy) - Math.abs(b - sy))
    .find(y => !hits(y) && !cuts(y))
  // ponytail: one detour through one row gap; a node dragged into an odd place may still be crossed.
  return fy === undefined ? straight : [[sx, sy], [gx, sy], [gx, fy], [cx, fy], [cx, ty], [tx, ty]]
}

/** Straight segments with rounded corners. */
function polyline(points: Point[], r = 10): string {
  const p = points.filter((q, k) => !k || q[0] !== points[k - 1][0] || q[1] !== points[k - 1][1])
  const toward = (a: Point, b: Point, d: number) => `${a[0] + Math.sign(b[0] - a[0]) * d},${a[1] + Math.sign(b[1] - a[1]) * d}`
  let d = `M${p[0][0]},${p[0][1]}`
  for (let k = 1; k < p.length - 1; k++) {
    const [a, b, c] = [p[k - 1], p[k], p[k + 1]]
    const q = Math.min(r, (Math.abs(b[0] - a[0]) + Math.abs(b[1] - a[1])) / 2, (Math.abs(c[0] - b[0]) + Math.abs(c[1] - b[1])) / 2)
    d += ` L${toward(b, a, q)} Q${b[0]},${b[1]} ${toward(b, c, q)}`
  }
  return d + ` L${p.at(-1)![0]},${p.at(-1)![1]}`
}

function River({ data, source, target, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition }: EdgeProps<Edge<LineData>>) {
  // Read live, so a route follows a node the user dragged.
  const nodes = useNodes()
  if (!data) return null
  const long = targetX - sourceX > GAP + W / 2
  const boxes = long
    ? nodes.filter(n => n.id !== source && n.id !== target).map(n => ({ x: n.position.x, y: n.position.y, w: n.width ?? W, h: n.height ?? H }))
    : []
  const d = long
    ? polyline(route([sourceX, sourceY], [targetX, targetY], boxes))
    : getBezierPath({ sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition })[0]
  return (
    <path
      d={d}
      className="river"
      data-state={data.state}
      data-also={data.also || undefined}
      data-off={data.off || undefined}
    />
  )
}
export const nodeTypes = { implement: WorkCard, review: WorkCard, gate: WorkCard, start: Dot }
export const edgeTypes = { river: River }
