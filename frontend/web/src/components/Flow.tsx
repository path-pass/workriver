// Layer 1, activity: the pieces of work handed to agents as a flow. Each is a node with
// something going in and something coming out; a line is one handing over to the next.
// It starts in one circle and has no end: a finished branch simply stops, in water colour.
import { applyNodeChanges, ControlButton, Controls, getBezierPath, Handle, MiniMap, Position, ReactFlow, useNodes, type Edge, type EdgeProps, type Node, type NodeChange, type NodeProps, type ReactFlowInstance } from '@xyflow/react'
import { AntVDagreLayout } from '@antv/layout'
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { ScrollArea } from '@/components/ui/scroll-area'
import { commitStats, ms, untested, finderLabel, findingsOf, openCount, related, reviewLabel, SEVERITIES, severityCounts, type Pick } from '@/data'
import { dropTask, record } from '@/runtime'
import { Input } from '@/components/ui/input'
import { contentLang, useT } from '@/i18n'
import { linkTo, pathOf } from '@/link'
import { cn } from '@/lib/utils'
import type { Finding, Unit, Task } from '@/types'
import { threadsOf, turnsOf, type Talk } from './Ask'
import { Decide, Decided, GateForm, GateSent, Thumbs } from './Inputs'
import { useAct, findingKey, gateKey } from '@/act'
import { Content, CopyButton, ExternalChip, Icon, Mark, Prose, Slot, SLOT, TerminalButton, Tip, Waiting, Who } from './bits'
import { RunSteps } from './Run'
import { Choice } from './Choice'

const W = 176, H = 48, R = 18

// ---- layout: a layered layout (dagre) places the nodes left to right once; React Flow
// draws the lines, so they follow a node when it is dragged ----

interface Line { id: string; source: string; target: string; primary: boolean }
interface Layout { unitId: string; h: number; pos: Map<string, { x: number; y: number }>; lines: Line[] }

const isStart = (id: string) => id === 'start'

async function layoutFlow(unit: Unit): Promise<Layout> {
  const ids = new Set(unit.tasks.map(n => n.id))
  const lines: Line[] = []
  for (const n of unit.tasks)
    for (const i of n.inputs)
      if ((i.from === 'start' || ids.has(i.from)) && !lines.some(l => l.source === i.from && l.target === n.id))
        lines.push({ id: `${i.from}>${n.id}`, source: i.from, target: n.id, primary: i.primary })
  // Top to bottom follows the data, each node ahead of what follows it, so that a review
  // stays next to the work it looked at.
  const order: string[] = []
  const walk = (id: string) => {
    order.push(id)
    unit.tasks.filter(n => (n.inputs.find(i => i.primary) ?? n.inputs[0])?.from === id).forEach(n => walk(n.id))
  }
  walk('start')
  for (const n of unit.tasks) if (!order.includes(n.id)) order.push(n.id)
  // A node's column is the number of hand-overs behind it: the longest path from the start.
  // dagre's longest-path ranker packs nodes towards the sinks, so the lines go in reversed
  // and the direction is flipped: the start is the only sink.
  const dagre = new AntVDagreLayout({
    rankdir: 'RL',
    ranker: 'longest-path',
    // The layout adds its own 50 between layers and triples nodesep: 56 across, 24 down.
    ranksep: 3,
    nodesep: 8,
    nodeOrder: order,
    edgeLabelSpace: false,
    nodeSize: (n: { id?: string }) => (isStart(String(n.id)) ? [R, R] : [W, H]),
  })
  await dagre.execute({ nodes: order.map(id => ({ id })), edges: lines.map(l => ({ id: l.id, source: l.target, target: l.source })) })
  const pos = new Map<string, { x: number; y: number }>()
  dagre.forEachNode(n => {
    const [w, h] = isStart(String(n.id)) ? [R, R] : [W, H]
    pos.set(String(n.id), { x: Math.round(n.x - w / 2), y: Math.round(n.y - h / 2) })
  })
  return { unitId: unit.id, h: Math.max(...[...pos.values()].map(p => p.y)) + H, pos, lines }
}

// ---- node and line types ----

type CardData = { node: Task; dim: boolean; picked: boolean; onPick: (id: string) => void; talks: number }
type DotData = { title: string }
type LineData = { state?: 'flowed' | 'flowing'; also: boolean; off: boolean }

const glyph = (n: Task) =>
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

/** A review's label with the reviewer's name, and punctuation right after it (「：」 may not start a line), kept whole. */
function ReviewLabel({ n }: { n: Task }) {
  const { t } = useT()
  const by = finderLabel(t, n), label = reviewLabel(t, n), i = label.indexOf(by)
  if (i < 0) return label
  const end = i + by.length + label.slice(i + by.length).match(/^\p{P}*/u)![0].length
  return <>{label.slice(0, i)}<span className="whitespace-nowrap">{label.slice(i, end)}</span>{label.slice(end)}</>
}

// What kind of work is a shape (set in CSS from data-kind): square corners for
// implementing, a pill for a review, a double frame for a gate. A review says how much it found per severity and
// carries a violet count while findings of it wait for the user.
function WorkCard({ data }: NodeProps<Node<CardData>>) {
  const { node: n, dim, picked, onPick, talks } = data
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
            ? n.result === 'found' ? <>{finderLabel(t, n)}<span className="sev">{severityCounts(findingsOf(n))}</span></> : <ReviewLabel n={n} />
            : n.state === 'unregistered' || !n.taskId ? <NodeName n={n} /> : <><b>#{n.taskId}</b> <Content text={n.title} /></>}
        </span>
        {n.agent && (n.state === 'running' || n.state === 'todo') && <Who name={n.agent} live={n.writing} />}
        {open > 0 && <Waiting count={open} />}
        {/* Questions asked about this work: information, not a button. */}
        {/* Work handed in with no check run: information, not a call to act. */}
        {untested(n) && <span className="untested"><Mark name="untested" label={t('evidence.none')} /></span>}
        {n.blind && <span className="blind"><Mark name="blind" label={t('task.blindHint')} /></span>}
        {n.state === 'dropped' && <span className="dropped"><Mark name="dropped" label={t('task.dropped')} /></span>}
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
const nodeTypes = { implement: WorkCard, review: WorkCard, gate: WorkCard, start: Dot }
const edgeTypes = { river: River }

// ---- the graph ----

// ---- where the view settles ----

/** Below this zoom a card's 12 px title drops under 9 px and stops being readable. */
const READABLE = 0.75
/** Room kept clear around the graph, in px: the zoom buttons at the top right (26 px wide,
 *  15 px from the edge; a card's badge sticks out 7 px) and React Flow's credit at the
 *  bottom right. The legend is a row below the canvas. */
const PAD = { top: 16, right: 56, bottom: 32, left: 16 }
/** The overview, at the bottom left: shown only when the graph does not fit. */
const MINI = { width: 240, height: 48 }

type Rect = { x0: number; y0: number; x1: number; y1: number }

/**
 * The whole graph when it fits at a readable zoom (centred, never above 1). Otherwise the
 * latest work at that zoom: the graph's right end against the right edge, moved right only as
 * far as it takes to show where `latest` starts, when all of `latest` fits (otherwise the
 * older part is the one cut); at the top when the height fits, else centred on `latest`. The
 * graph is never pulled away from its own ends; the rest is a pan away.
 */
function settle(all: Rect, latest: Rect, w: number, h: number): { x: number; y: number; zoom: number; whole: boolean } {
  const rw = w - PAD.left - PAD.right, rh = h - PAD.top - PAD.bottom
  const bw = all.x1 - all.x0, bh = all.y1 - all.y0
  const fit = Math.min(rw / bw, rh / bh, 1)
  const centre = (z: number, p: number, room: number, a0: number, size: number) => p + (room - size * z) / 2 - a0 * z
  if (fit >= READABLE) return { x: centre(fit, PAD.left, rw, all.x0, bw), y: centre(fit, PAD.top, rh, all.y0, bh), zoom: fit, whole: true }
  const z = READABLE
  // Between showing the graph's start at the left edge and its end at the right one.
  const keep = (want: number, p0: number, size: number, p1: number, a0: number, a1: number) => Math.min(p0 - a0 * z, Math.max(size - p1 - a1 * z, want))
  const end = w - PAD.right - all.x1 * z
  const x = bw * z <= rw ? centre(z, PAD.left, rw, all.x0, bw)
    : keep((latest.x1 - latest.x0) * z <= rw ? Math.max(end, PAD.left - latest.x0 * z) : end, PAD.left, w, PAD.right, all.x0, all.x1)
  const y = bh * z <= rh ? PAD.top - all.y0 * z : keep(PAD.top + rh / 2 - ((latest.y0 + latest.y1) / 2) * z, PAD.top, h, PAD.bottom, all.y0, all.y1)
  return { x, y, zoom: z, whole: false }
}

/** The box around the given nodes as the layout placed them. */
function rectOf(layout: Layout, ids: string[]): Rect {
  const r = { x0: Infinity, y0: Infinity, x1: -Infinity, y1: -Infinity }
  for (const id of ids) {
    const p = layout.pos.get(id)
    if (!p) continue
    const [w, h] = isStart(id) ? [R, R] : [W, H]
    r.x0 = Math.min(r.x0, p.x); r.y0 = Math.min(r.y0, p.y); r.x1 = Math.max(r.x1, p.x + w); r.y1 = Math.max(r.y1, p.y + h)
  }
  return r
}

/** The latest work: what is running or waits for the user, else the last column. */
function latestOf(unit: Unit, layout: Layout): string[] {
  const live = unit.tasks.filter(n => n.state === 'running' || n.result === 'running' || (n.kind === 'gate' && n.state === 'waiting')).map(n => n.id)
  if (live.some(id => layout.pos.has(id))) return live
  const end = Math.max(...[...layout.pos.values()].map(p => p.x))
  return [...layout.pos].filter(([, p]) => p.x === end).map(([id]) => id)
}

/** React Flow's own fit icon: the button is ours, so that it settles the view our way. */
const FitIcon = (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 30" aria-hidden>
    <path d="M3.692 4.63c0-.53.4-.938.939-.938h5.215V0H4.708C2.13 0 0 2.054 0 4.63v5.216h3.692V4.631zM27.354 0h-5.2v3.692h5.17c.53 0 .984.4.984.939v5.215H32V4.631A4.624 4.624 0 0027.354 0zm.954 24.83c0 .532-.4.94-.939.94h-5.215v3.768h5.215c2.577 0 4.631-2.13 4.631-4.707v-5.139h-3.692v5.139zm-23.677.94c-.531 0-.939-.4-.939-.94v-5.138H0v5.139c0 2.577 2.13 4.707 4.708 4.707h5.138V25.77H4.631z" />
  </svg>
)

/** A node's colour in the overview, by the same state as on the graph. */
const miniClass = (n: Node) => {
  if (n.type === 'start') return 'mini-start'
  const task = (n.data as CardData).node
  return task.state === 'dropped' ? '' : task.kind === 'gate' && task.state === 'waiting' ? 'mini-you' : task.kind !== 'review' && (task.state === 'done' || task.state === 'running') ? `mini-${task.state}` : ''
}

function Graph({ unit, picked, finding, onPick, talk, fitKey }: { unit: Unit; picked: string | null; finding?: string; onPick: (id: string | null) => void; talk: Talk; fitKey: unknown }) {
  const { t } = useT()
  const flow = useRef<ReactFlowInstance<Node, Edge<LineData>> | null>(null)
  const box = useRef<HTMLDivElement>(null)
  // Once the user moves the view (drag, pinch, the buttons, the overview), it is theirs until
  // they pick another unit or press the fit button. `ours` tells our own moves apart: a
  // viewport set without a duration moves at once, inside the call.
  const moved = useRef(false)
  const ours = useRef(false)
  const [mini, setMini] = useState(false)
  const [layout, setLayout] = useState<Layout | null>(null)
  const latest = useRef(unit)
  latest.current = unit
  const shownLayout = useRef(layout)
  shownLayout.current = layout
  const place = useCallback((force = false) => {
    const l = shownLayout.current, inst = flow.current, el = box.current
    if (!l || !inst || !el || l.unitId !== latest.current.id || (moved.current && !force)) return
    moved.current = false
    const v = settle(rectOf(l, [...l.pos.keys()]), rectOf(l, latestOf(latest.current, l)), el.clientWidth, el.clientHeight)
    ours.current = true
    inst.setViewport({ x: v.x, y: v.y, zoom: v.zoom })
    ours.current = false
    setMini(!v.whole)
  }, [])
  // Another unit: a fresh view, the user's moves forgotten.
  useEffect(() => { flow.current = null; moved.current = false }, [unit.id])
  // When the room around the graph changes (the rail folds, a panel's drag ends, the window
  // is resized), the view settles again; and once more when the overview comes or goes, since
  // the canvas then takes the zoomed graph's height and room for the overview below it.
  useEffect(() => {
    let later = setTimeout(place, 80)
    const again = () => { clearTimeout(later); later = setTimeout(place, 150) }
    window.addEventListener('resize', again)
    return () => { clearTimeout(later); window.removeEventListener('resize', again) }
  }, [fitKey, place])
  useEffect(() => { place() }, [mini, place])
  // Laid out again only when the graph's shape changes, not on every refresh of the data.
  const shape = unit.id + '|' + unit.tasks.map(n => `${n.id}<${n.inputs.map(i => i.from + (i.primary ? '' : '~')).join(',')}`).join(';')
  useEffect(() => {
    let current = true
    layoutFlow(latest.current).then(l => current && setLayout(l))
    return () => { current = false }
  }, [shape])
  // A new layout (the graph grew, or another unit) settles the view, unless the user moved it.
  useEffect(() => { place() }, [layout, place])

  const { nodes: placed, edges } = useMemo(() => {
    if (!layout || layout.unitId !== unit.id) return { nodes: [], edges: [] }
    const lit = picked ? related(unit, picked, finding) : null
    const isLit = (id: string) => !lit || id === 'start' || lit.has(id)
    const byId = new Map(unit.tasks.map(n => [n.id, n]))
    const at = (id: string) => layout.pos.get(id) ?? { x: 0, y: 0 }
    const fixed = { selectable: false, focusable: false }
    const nodes: Node[] = [
      { id: 'start', type: 'start', position: at('start'), width: R, height: R, data: { title: unit.purpose || t('flow.start') } satisfies DotData, ...fixed },
      // A node the layout has not placed yet (it just arrived) waits for the next layout.
      ...unit.tasks.filter(n => layout.pos.has(n.id)).map(n => ({
        id: n.id, type: n.kind, position: at(n.id), width: W, height: H, ...fixed,
        data: { node: n, dim: !isLit(n.id), picked: n.id === picked, onPick, talks: turnsOf(n, talk).length } satisfies CardData,
      })),
    ]
    // A line takes the state of the work it leads into.
    const stateOf = (l: Line): LineData['state'] => {
      const n = byId.get(l.target)
      if (!n) return undefined
      return n.state === 'running' ? 'flowing' : n.state === 'done' || n.state === 'unregistered' ? 'flowed' : undefined
    }
    const rank = (e: Edge<LineData>) => (e.data!.off ? 0 : e.data!.state === 'flowing' ? 3 : e.data!.state === 'flowed' ? 2 : 1)
    const edges: Edge<LineData>[] = layout.lines
      .map(l => ({
        id: l.id, source: l.source, target: l.target, type: 'river', selectable: false, focusable: false,
        data: { state: stateOf(l), also: !l.primary, off: !(isLit(l.source) && isLit(l.target)) },
      }))
      .sort((a, b) => rank(a) - rank(b))
    return { nodes, edges }
  }, [layout, unit, picked, finding, onPick, talk, t])

  // React Flow owns where nodes are once they are on screen, so that dragging works: its
  // changes are applied here, and a re-render for a new pick keeps what the user moved. A new
  // layout (another unit, or the graph grew) places everything afresh.
  const [nodes, setNodes] = useState<Node[]>([])
  const shown = useRef<Layout | null>(null)
  useEffect(() => {
    const sameLayout = shown.current === layout
    shown.current = placed.length ? layout : null
    setNodes(prev => {
      const old = new Map(sameLayout ? prev.map(n => [n.id, n]) : [])
      return placed.map(n => { const o = old.get(n.id); return o ? { ...n, position: o.position, measured: o.measured } : n })
    })
  }, [placed, layout])
  const onNodesChange = useCallback((changes: NodeChange[]) => setNodes(ns => applyNodeChanges(changes, ns)), [])

  // The graph sits in a window of its own: drag the ground to move around, pinch or use
  // the buttons to zoom. The wheel is left alone so the page still scrolls.
  const ready = layout && layout.unitId === unit.id
  return (
    <>
    <div ref={box} className="flow -mx-4" style={{ height: ready ? `min(${Math.round(mini ? layout.h * READABLE + 48 + MINI.height : layout.h + 48)}px, 70vh)` : 240 }}>
      {ready && (
        <ReactFlow
          key={unit.id}
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          onNodesChange={onNodesChange}
          // A keyboard click (detail 0) is the card's own; taking it here too would pick and unpick.
          onNodeClick={(e, node) => { if (node.type !== 'start' && e.detail !== 0) onPick(node.id) }}
          onPaneClick={() => onPick(null)}
          nodesConnectable={false}
          nodesFocusable={false}
          edgesFocusable={false}
          elementsSelectable={false}
          nodeDragThreshold={3}
          zoomOnScroll={false}
          zoomOnDoubleClick={false}
          preventScrolling={false}
          onInit={inst => { flow.current = inst; place() }}
          onMoveStart={() => { if (!ours.current) moved.current = true }}
          minZoom={0.3}
          maxZoom={1.5}
          ariaLabelConfig={{
            'controls.ariaLabel': t('flow.controls'),
            'controls.zoomIn.ariaLabel': t('flow.zoomIn'),
            'controls.zoomOut.ariaLabel': t('flow.zoomOut'),
            'minimap.ariaLabel': t('flow.overview'),
          }}
        >
          <Controls showInteractive={false} showFitView={false} position="top-right">
            <ControlButton className="react-flow__controls-fitview" onClick={() => place(true)} title={t('flow.fit')} aria-label={t('flow.fit')}>{FitIcon}</ControlButton>
          </Controls>
          {/* Only when the graph does not fit: where the view is in the whole, and a way there. */}
          {mini && (
            <MiniMap
              position="bottom-left"
              style={MINI}
              nodeClassName={miniClass}
              nodeBorderRadius={4}
              pannable
              onClick={(_, at) => flow.current?.setCenter(at.x, at.y, { zoom: flow.current.getZoom() })}
            />
          )}
        </ReactFlow>
      )}
    </div>
    {/* Below the canvas, never over it: no node can hide behind it. */}
    <Legend />
    </>
  )
}

// ---- what went into the picked piece of work and what came out of it ----

/** What was added to a finding after it was reported: who, when, what; oldest first. */
function FindingNotes({ f, id = false }: { f: Finding; id?: boolean }) {
  const { t, when } = useT()
  if (!f.notes?.length) return null
  return (
    <ul className="mt-1.5 flex flex-col gap-1 text-xs">
      {f.notes.map((n, k) => (
        <li key={k} className="flex items-start gap-1.5">
          <Who name={n.by} />
          <span className="min-w-0">
            <span className="text-ink-2">{id && <span className="font-mono font-semibold">{f.id} </span>}{t('finding.noted', { by: n.by })} · {when(ms(n.at))}</span>
            <span className="block whitespace-pre-line wrap-anywhere" lang={contentLang(n.text)}>{n.text}</span>
            <Thumbs list={n.attachments ?? []} />
          </span>
        </li>
      ))}
    </ul>
  )
}

/** One finding of the picked review: a line to scan, its details behind the chevron. */
function FindingRow({ unit, n, f, picked, onSelect, onPick }: { unit: Unit; n: Task; f: Finding; picked: boolean; onSelect: () => void; onPick: (id: string) => void }) {
  const { t } = useT()
  const act = useAct()
  const row = useRef<HTMLDivElement>(null)
  // Arriving from the work a finding caused, the finding may sit below the fold of the list.
  useEffect(() => { if (picked) row.current?.scrollIntoView({ block: 'nearest' }) }, [picked])
  const quiet = f.state !== 'open' && f.state !== 'accepted' && f.state !== 'revised'
  const suggests = f.state === 'open' && f.recommendation ? t('finding.suggests', { decision: t(`finding.decision.${f.recommendation.decision}`) }) : ''
  return (
    <Collapsible ref={row} className="frow group" data-picked={picked || undefined}>
      <div className="flex items-start gap-1">
        <CollapsibleTrigger className="fchev" aria-label={t('finding.details', { id: f.id })}><Icon name="chev" /></CollapsibleTrigger>
        <button type="button" className="flex min-w-0 flex-1 items-baseline gap-2 rounded-sm py-1 text-left" aria-pressed={picked} onClick={onSelect}>
          <span className="flex-none font-mono text-xs font-semibold">{f.id}</span>
          <span className={cn('truncate group-data-[state=open]:whitespace-pre-line', quiet && 'text-ink-2', f.state === 'rejected' && 'line-through')} lang={contentLang(f.problem)}>{f.problem}</span>
        </button>
        {/* What was added to it since: a small count; the additions are in its details. */}
        {(f.notes?.length ?? 0) > 0 && (
          <Tip text={t('finding.notes', { count: f.notes!.length })}>
            <span className="flex h-7 flex-none items-center gap-0.5 text-xs text-ink-2"><Icon name="plus" className="size-3" /><span aria-hidden>{f.notes!.length}</span><span className="sr-only">{t('finding.notes', { count: f.notes!.length })}</span></span>
          </Tip>
        )}
        {f.decidedBy === 'agent' && (
          <span className="flex h-7 items-center text-ink-2"><Mark name="agent" label={t('finding.byAgent')} /></span>
        )}
        {/* Revised: like accepted, the user's way; a pencil says so. */}
        {f.state === 'revised' && <span className="flex h-7 items-center text-ink-2"><Mark name="pen" label={t('finding.revised')} /></span>}
        {(f.state === 'accepted' || f.state === 'revised') && !f.leadsTo.length ? (
          // Decided here and not turned into tasks yet: the manager bot has it.
          <span className="flex flex-none items-center py-1 text-xs text-ink-2">{t(f.state === 'revised' ? 'finding.revised' : 'finding.acceptedHere')}</span>
        ) : f.state === 'accepted' || f.state === 'revised' ? (
          <span role="group" aria-label={t('finding.leadsTo')} className="flex flex-none items-center gap-1 py-0.5">
            {f.leadsTo.map(id => {
              const to = unit.tasks.find(n => n.id === id)
              return to && (
                <Tip key={id} text={<span><NodeName n={to} /></span>}>
                  <button type="button" className="chip" data-state={to.state} onClick={() => onPick(id)}>{to.taskId ? `#${to.taskId}` : <Content text={to.title} />}</button>
                </Tip>
              )
            })}
          </span>
        ) : (
          <span className={cn('flex flex-none items-center gap-1 py-1 text-xs', f.state === 'open' && f.decider !== 'manager' ? 'text-you-ink' : 'text-ink-2')}>
            <ExternalChip ext={f.external} />
            {(f.state === 'deferred' || f.state === 'scheduled') && <Icon name="later" className="size-3.5" />}
            {suggests && <span className="text-ink-2 max-[700px]:hidden">{suggests} ·</span>}
            {/* Handed to the intent's manager by the user: open, but not waiting for the user. */}
            {f.state === 'open' && f.decider === 'manager' ? <><Icon name="manager" className="size-3.5" />{t('finding.managerDecides')}</>
              : f.state === 'open' && f.noManager ? t('finding.noManager')
              : t(`finding.${f.state}`)}
          </span>
        )}
      </div>
      <CollapsibleContent className="pb-1.5 pl-6 text-xs whitespace-pre-line">
        {suggests && <div><span className="text-ink-2">{t('io.field', { name: suggests })}</span><span lang={contentLang(f.recommendation?.reason)}>{f.recommendation?.reason}</span></div>}
        {f.reason && <div><span className="text-ink-2">{t('io.field', { name: t('finding.dropped') })}</span><span lang={contentLang(f.reason)}>{f.reason}</span></div>}
        {f.impact && <div><span className="text-ink-2">{t('io.field', { name: t('io.impact') })}</span><span lang={contentLang(f.impact)}>{f.impact}</span></div>}
        {f.evidence.length > 0 && <div><span className="text-ink-2">{t('io.field', { name: t('io.evidence') })}</span><span className="font-mono">{f.evidence.join(t('list.sep'))}</span></div>}
        <Thumbs list={f.attachments ?? []} />
        <FindingNotes f={f} />
        {f.state === 'open' ? <Decide unit={unit} task={n} f={f} /> : act.delivery[findingKey(unit, n, f)] || f.delivery ? <Decided unit={unit} task={n} f={f} /> : null}
        {/* The manager's decision stands until the user makes another: theirs overrides it. */}
        {f.decidedBy === 'agent' && ['accepted', 'revised', 'rejected', 'deferred'].includes(f.state) && (
          <div className="mt-2">
            <span className="text-ink-2">{t('finding.override')}</span>
            <Decide unit={unit} task={n} f={f} />
          </div>
        )}
      </CollapsibleContent>
    </Collapsible>
  )
}

/** What a review or a gate found: grouped by severity, scrolling inside the panel. */
function Findings({ unit, n, finding, onPick }: { unit: Unit; n: Task; finding?: string; onPick: (p: Pick) => void }) {
  const { num } = useT()
  const found = findingsOf(n)
  return (
    // Radix lays the content out as a table, which would let a long line widen it instead
    // of being cut: make it a block.
    <ScrollArea className="-my-1 -mr-2 [&>[data-slot=scroll-area-viewport]]:max-h-[50vh] [&>[data-slot=scroll-area-viewport]>div]:block!">
      <div className="py-1 pr-3">
        {SEVERITIES.map(s => {
          const list = found.filter(f => f.severity === s)
          return list.length > 0 && (
            <section key={s} className="fgroup">
              <h4 className="flex items-baseline gap-1.5 text-xs"><b className="font-mono">{s}</b><span className="text-ink-2">{num(list.length)}</span></h4>
              {list.map(f => (
                <FindingRow
                  key={f.id}
                  unit={unit}
                  n={n}
                  f={f}
                  picked={f.id === finding}
                  onSelect={() => onPick({ kind: 'task', id: n.id, finding: f.id === finding ? undefined : f.id })}
                  onPick={id => onPick({ kind: 'task', id })}
                />
              ))}
            </section>
          )
        })}
      </div>
    </ScrollArea>
  )
}

/** The checks run on a piece of work, one line each; with none, done work says so quietly. */
function Checks({ n }: { n: Task }) {
  const { t } = useT()
  const list = n.output.evidence ?? []
  if (!list.length) return untested(n) ? <div className="mt-2 flex items-center gap-1.5 text-xs text-ink-2"><Icon name="untested" className="size-3.5" />{t('evidence.none')}</div> : null
  return (
    <ul className="mt-2 flex flex-col gap-1" aria-label={t('evidence.label')}>
      {list.map((e, k) => (
        <li key={k} className="check" data-result={e.result}>
          <Icon name={e.result === 'passed' ? 'check' : 'no'} className="size-3.5" />
          <span className="sr-only">{t(`evidence.${e.result}`)}</span>
          <code className="min-w-0 truncate">{e.command}</code>
          <CopyButton text={e.command} />
          <span className="flex-none text-ink-2" lang={contentLang(e.summary)}>{e.summary}</span>
          {e.attachment && <Thumbs list={[e.attachment]} />}
        </li>
      ))}
    </ul>
  )
}

function Detail({ unit, n, finding, onPick, talk }: { unit: Unit; n: Task; finding?: string; onPick: (p: Pick) => void; talk: Talk }) {
  const { t, num, when } = useT()
  const byId = (id?: string) => unit.tasks.find(x => x.id === id)
  const commitsOf = (x?: Task) => unit.commits.filter(c => x?.output.commits?.includes(c.id))
  const quiet = (text: string) => <div className="text-xs text-ink-2">{text}</div>
  const paths = (list: string[]) => <div className="font-mono text-xs text-ink-2">{list.join(t('list.sep'))}</div>
  const threads = threadsOf(n, talk).length
  const found = findingsOf(n).length > 0 && <Findings unit={unit} n={n} finding={finding} onPick={onPick} />
  let head: ReactNode, input: ReactNode, output: ReactNode
  if (n.kind === 'review') {
    const of = byId(n.inputs.find(i => i.primary)?.from)
    const files = [...new Set(commitsOf(of).flatMap(c => c.files.map(f => f.path)))]
    head = <><Icon name={glyph(n)} />{finderLabel(t, n)} · {t(n.state === 'dropped' ? 'state.dropped' : n.result === 'running' ? 'state.running' : 'state.done')}</>
    input = <>{of && <div><Slot text={t('io.changesOf', { label: SLOT })}><NodeName n={of} /></Slot></div>}{files.length ? paths(files) : quiet(t('io.noChanges'))}</>
    output = <>{found || quiet(t(n.result === 'running' ? 'io.noFindingsYet' : 'io.noFindings'))}<Checks n={n} /><Thumbs list={n.output.attachments ?? []} /></>
  } else if (n.kind === 'gate') {
    head = <><Icon name={glyph(n)} />{finderLabel(t, n)} · {t(`state.${n.state}`)}</>
    // Whoever did the work the gate follows wrote its steps (the sample's guess; the service knows).
    input = <><Prose text={n.input.text || n.title} /><RunSteps steps={n.input.steps ?? []} ask={{ unit: unit.id, from: `${unit.id}#${n.id}`, by: byId(n.inputs.find(i => i.primary)?.from)?.agent ?? undefined }} /></>
    output = n.state === 'waiting'
      ? <GateForm key={gateKey(unit, n)} unit={unit} gate={n} />
      : <>
          {n.output.note && <Prose text={n.output.note} />}
          <Thumbs list={n.output.attachments ?? []} />
          <GateSent unit={unit} gate={n} />
          {found && <div className="mt-3">{found}</div>}
        </>
  } else {
    const commits = commitsOf(n)
    const causes = n.input.causes.flatMap(c => {
      const by = byId(c.review)
      return by ? findingsOf(by).filter(f => c.findings.includes(f.id)).map(f => ({ by, f })) : []
    })
    head = <>{n.agent ? <Who name={n.agent} /> : <Icon name={glyph(n)} />}{[t(n.design ? 'kind.design' : n.research ? 'kind.research' : 'kind.implement'), n.agent, t(`state.${n.state}`)].filter(Boolean).join(' · ')}</>
    input = <>
      <Prose text={n.input.text || n.title} />
      {n.state === 'unregistered' && quiet(t('io.unknownInput'))}
      {/* The gap is closed by saying which task the commit belongs to; nothing is rewritten. */}
      {n.state === 'unregistered' && n.output.commits?.[0] && (
        <label className="mt-2 flex items-center gap-2 text-xs">
          {t('io.linkTo')}
          <Choice
            label={t('io.linkTo')}
            value=""
            placeholder="…"
            onChange={v => { record('commit.linked', { unit: unit.id, hash: n.output.commits![0], task: Number(v) }).catch(err => console.warn(err)) }}
            options={unit.tasks.filter(x => x.taskId && x.kind !== 'gate').map(x => ({ value: x.id, label: `#${x.taskId} ${x.title || finderLabel(t, x)}`, lang: x.title ? contentLang(x.title) : undefined }))}
            className="h-7 w-auto max-w-[60%] text-xs"
          />
        </label>
      )}
      {causes.length > 0 && (
        <div className="mt-2">
          <h4 className="text-xs text-ink-2">{t('finding.causes')}</h4>
          {causes.map(({ by, f }) => (
            <button key={by.id + f.id} type="button" className="cause" onClick={() => onPick({ kind: 'task', id: by.id, finding: f.id })}>
              <Icon name={glyph(by)} />
              <span className="flex-none">{finderLabel(t, by)}</span>
              <span className="flex-none font-mono text-xs font-semibold">{f.id}</span>
              <span className="min-w-0 whitespace-pre-line" lang={contentLang(f.problem)}>{f.problem}</span>
            </button>
          ))}
          {/* What was added to those findings later is part of what this work must meet. */}
          {causes.map(({ by, f }) => <FindingNotes key={by.id + f.id} f={f} id />)}
          <Thumbs list={causes.flatMap(c => c.f.attachments ?? [])} />
        </div>
      )}
    </>
    // Delivered work shows its note and checks even without a commit of its own.
    output = <>
      {commits.length
        ? commits.map(c => {
            const s = commitStats(c)
            return (
              <div key={c.id}>
                {c.live ? t('io.uncommitted') : <span lang={contentLang(c.subject)}>{c.subject}</span>}
                <span className="text-xs text-ink-2"> · <span className="whitespace-nowrap">{t('stat.files', { count: s.files })}</span> +{num(s.add)} −{num(s.del)}{c.linked && ` · ${t('io.linked')}`}{c.rebasedFrom && ` · ${t('io.rebasedFrom', { hash: c.rebasedFrom })}`}</span>
              </div>
            )
          })
        : (n.lostCommits?.length ?? 0) > 0 ? quiet(t('io.lostCommits', { commits: n.lostCommits!.join(t('list.sep')) }))
        : n.state !== 'done' && <>{quiet(t('io.notStarted'))}{n.plans.length > 0 && <div className="font-mono text-xs text-ink-2">{t('io.willChange', { paths: n.plans.join(t('list.sep')) })}</div>}</>}
      {n.output.note && <div className="mt-1"><Prose text={n.output.note} /></div>}
      <Thumbs list={n.output.attachments ?? []} />
      <Checks n={n} />
    </>
  }
  return (
    <div className="mt-1 overflow-hidden rounded-md border border-you">
      <div className="flex items-center gap-1.5 bg-you-bg px-3 py-2 text-sm font-semibold text-you-ink [--who-ground:transparent]">
        {head}
        {/* Blind: its findings were made without seeing other work; weigh agreement accordingly. */}
        {n.blind && <Tip text={t('task.blindHint')}><span className="flex items-center gap-1 font-normal"><Icon name="blind" className="size-3.5" />{t('task.blind')}</span></Tip>}
        <span className="ml-auto flex items-center gap-1">
          {/* The agent on work that is running or still to do, while its session lives: to its terminal. */}
          {n.agent && (n.state === 'running' || n.state === 'todo') && talk.sessions.some(s => s.agent === n.agent && !s.ended) && (
            <TerminalButton agent={n.agent} session={n.agentSession} className="text-you-ink hover:bg-surface/60" />
          )}
          {/* Its own link: to send someone straight here. */}
          <CopyButton text={linkTo(pathOf(unit.id, { kind: 'task', id: n.id }))} aria={t('link.copy')} className="text-you-ink hover:bg-surface/60" />
        </span>
      </div>
      {/* No longer needed: by whom and when, then why in their words. */}
      {n.dropped && (
        <div className="flex flex-col gap-0.5 border-b border-line px-3 py-2 text-sm wrap-anywhere">
          <span className="flex items-center gap-1.5 text-xs text-ink-2">
            <Icon name="dropped" className="size-3.5" />
            {n.dropped.by === 'user' ? t('task.droppedByYou', { when: when(ms(n.dropped.at)) }) : t('task.droppedBy', { by: n.dropped.by, when: when(ms(n.dropped.at)) })}
          </span>
          <span className="whitespace-pre-line" lang={contentLang(n.dropped.reason)}>{n.dropped.reason}</span>
        </div>
      )}
      <div className={cn('grid text-sm wrap-anywhere', n.kind === 'review' ? 'min-[701px]:grid-cols-[1fr_2fr]' : 'grid-cols-[repeat(auto-fit,minmax(260px,1fr))]')}>
        <div className="min-w-0 p-3"><h3 className="mb-1.5 text-xs font-semibold text-ink-2">{t('io.input')}</h3>{input}</div>
        <div className="min-w-0 border-line p-3 max-[700px]:border-t min-[701px]:border-l">
          <h3 className="mb-1.5 flex items-baseline gap-2 text-xs font-semibold text-ink-2">
            {t('io.output')}
            {/* Corrected after delivery: what shows is the latest. */}
            {n.amended && <span className="font-normal">{t('io.amended', { when: when(ms(n.amended)) })}</span>}
          </h3>
          {output}
        </div>
      </div>
      {/* Asking why happens in the side panel; here only the way there, with how many conversations there are. */}
      <div className="flex items-center gap-2 border-t border-line px-3 py-2 text-xs text-ink-2">
        <Button variant="outline" size="xs" className="rounded-sm" onClick={talk.onOpen}><Icon name="talk" className="size-3" />{t('ask.why')}</Button>
        {threads > 0 && <span>{t('ask.conversations', { count: threads })}</span>}
        {(n.state === 'todo' || n.state === 'running' || n.state === 'waiting') && <DropTask key={unit.id + n.id} unit={unit} n={n} />}
      </div>
    </div>
  )
}

/** The user marks work not done yet as no longer needed, saying why; nothing is deleted. */
function DropTask({ unit, n }: { unit: Unit; n: Task }) {
  const { t } = useT()
  const [open, setOpen] = useState(false)
  const [reason, setReason] = useState('')
  const [error, setError] = useState('')
  if (!open) return <Button variant="ghost" size="xs" className="ml-auto rounded-sm" onClick={() => setOpen(true)}><Icon name="dropped" className="size-3" />{t('task.drop')}</Button>
  const submit = () => { dropTask(unit, n, reason.trim()).then(() => setOpen(false), (e: Error) => setError(e.message)) }
  return (
    <form className="ml-auto flex min-w-0 flex-1 flex-wrap items-center justify-end gap-2" onSubmit={e => { e.preventDefault(); if (reason.trim()) submit() }}>
      <Input autoFocus aria-label={t('task.dropReason')} placeholder={t('task.dropReasonHint')} value={reason} onChange={e => setReason(e.target.value)} className="h-7 min-w-48 flex-1 text-xs" />
      <Button type="submit" size="xs" className="rounded-sm" disabled={!reason.trim()}>{t('task.dropConfirm')}</Button>
      <Button type="button" variant="ghost" size="xs" className="rounded-sm" onClick={() => { setOpen(false); setError('') }}>{t('task.dropCancel')}</Button>
      {error && <p role="alert" className="w-full text-xs wrap-anywhere text-del">{t('task.dropFailed', { error })}</p>}
    </form>
  )
}

function Legend() {
  const { t, num } = useT()
  const sw = (state: string, text: string) => <span className="key"><i className="sw" data-state={state} />{text}</span>
  const stroke = (state: string, text: string) => <span className="key"><i className="stroke" data-state={state} />{text}</span>
  const mark = (name: string, text: string) => <span className="key"><Icon name={name} />{text}</span>
  return (
    <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-ink-2">
      {sw('done', t('legend.done'))}{sw('running', t('legend.running'))}{sw('', t('legend.todo'))}{sw('review', t('kind.review'))}{sw('gate', t('kind.gate'))}<span className="key"><i className="count" aria-hidden>{num(1)}</i>{t('legend.waiting')}</span>
      {mark('security-review', t('reviewer.security-review'))}{mark('code-review', t('reviewer.code-review'))}{mark('test', t('reviewer.test'))}
      {stroke('flowed', t('legend.flowed'))}{stroke('flowing', t('legend.flowing'))}{stroke('also', t('legend.also'))}
    </div>
  )
}

/** `title` and `hint`: another heading than "Activity", with a quiet line under it. */
export function Activity({ unit, pick, onPick, talk, fitKey, title, hint }: { unit: Unit; pick: Pick; onPick: (p: Pick) => void; talk: Talk; fitKey: unknown; title?: string; hint?: string }) {
  const { t } = useT()
  const pickedId = pick?.kind === 'task' ? pick.id : null
  const picked = unit.tasks.find(n => n.id === pickedId)
  const pickNode = useMemo(() => (id: string | null) => onPick(id ? { kind: 'task', id } : null), [onPick])
  return (
    <section className="block-box">
      <h2 className="mb-2 text-lg font-[650]">{title ?? t('block.activity')}</h2>
      {hint && <p className="-mt-1 mb-2 text-sm text-ink-2">{hint}</p>}
      {unit.tasks.length ? (
        <>
          <Graph unit={unit} picked={pickedId} finding={pick?.finding} onPick={pickNode} talk={talk} fitKey={fitKey} />
          {picked && <Detail unit={unit} n={picked} finding={pick?.finding} onPick={onPick} talk={talk} />}
        </>
      ) : <p className="text-sm text-ink-2">{t('flow.empty')}</p>}
    </section>
  )
}
