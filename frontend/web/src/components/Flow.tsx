// Layer 1, activity: the pieces of work handed to agents as a flow. Each is a node with
// something going in and something coming out; a line is one handing over to the next.
// It starts in one circle and has no end: a finished branch simply stops, in water colour.
import { applyNodeChanges, Controls, getBezierPath, Handle, Position, ReactFlow, useNodes, type Edge, type EdgeProps, type Node, type NodeChange, type NodeProps, type ReactFlowInstance, type FitViewOptions } from '@xyflow/react'
import { AntVDagreLayout } from '@antv/layout'
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { ScrollArea } from '@/components/ui/scroll-area'
import { commitStats, ms, untested, finderLabel, findingsOf, nodeLabel, openCount, related, reviewLabel, SEVERITIES, severityCounts, type Pick } from '@/data'
import { record } from '@/runtime'
import { useT } from '@/i18n'
import { cn } from '@/lib/utils'
import type { Finding, Unit, Task } from '@/types'
import { threadsOf, turnsOf, type Talk } from './Ask'
import { Decide, Decided, GateForm, GateSent, Thumbs } from './Inputs'
import { useAct, findingKey, gateKey } from '@/act'
import { CopyButton, ExternalChip, Icon, Mark, Steps, TerminalButton, Tip, Waiting, Who } from './bits'
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
  : ({ done: 'check', running: 'pen', unregistered: 'no', todo: 'ring', waiting: 'ring' })[n.state]

const ports = (
  <>
    <Handle type="target" position={Position.Left} isConnectable={false} />
    <Handle type="source" position={Position.Right} isConnectable={false} />
  </>
)

// What kind of work is a shape (set in CSS from data-kind): square corners for
// implementing, a pill for a review, a double frame for a gate. A review says how much it found per severity and
// carries a violet count while findings of it wait for the user.
function WorkCard({ data }: NodeProps<Node<CardData>>) {
  const { node: n, dim, picked, onPick, talks } = data
  const { t, num } = useT()
  const open = openCount(n)
  return (
    <>
      <Tip text={nodeLabel(t, n)}>
      <button
        type="button"
        className="wnode"
        data-kind={n.kind}
        data-state={n.kind === 'review' ? undefined : n.state}
        data-dim={dim || undefined}
        aria-pressed={picked}
        // The mouse picks through React Flow's node click, which a drag does not trigger;
        // this handler is for the keyboard (detail 0).
        onClick={e => { if (e.detail === 0) onPick(n.id) }}
      >
        <Icon
          name={glyph(n)}
          className={n.kind === 'review' ? (n.result === 'running' ? 'text-amber!' : 'text-water!') : ''}
        />
        <span className="lb">
          {n.kind === 'review'
            ? n.result === 'found' ? <>{finderLabel(t, n)}<span className="sev">{severityCounts(findingsOf(n))}</span></> : reviewLabel(t, n)
            : n.state === 'unregistered' || !n.taskId ? nodeLabel(t, n) : <><b>#{n.taskId}</b> {n.title}</>}
        </span>
        {n.agent && (n.state === 'running' || n.state === 'todo') && <Who name={n.agent} live={n.writing} />}
        {open > 0 && <Waiting count={open} />}
        {/* Questions asked about this work: information, not a button. */}
        {/* Work handed in with no check run: information, not a call to act. */}
        {untested(n) && <span className="untested"><Mark name="untested" label={t('evidence.none')} /></span>}
        {n.blind && <span className="blind"><Mark name="blind" label={t('task.blindHint')} /></span>}
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
      <Tip text={data.title}><div className="wdot" /></Tip>
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

// Room at the bottom for the library's credit only: the legend is a row below the canvas.
const FIT: FitViewOptions = { padding: { top: '16px', right: '24px', bottom: '32px', left: '16px' }, maxZoom: 1 }

function Graph({ unit, picked, finding, onPick, talk, fitKey }: { unit: Unit; picked: string | null; finding?: string; onPick: (id: string | null) => void; talk: Talk; fitKey: unknown }) {
  const { t } = useT()
  // When the room around the graph changes (the rail folds), the graph fits it again.
  const flow = useRef<ReactFlowInstance<Node, Edge<LineData>> | null>(null)
  useEffect(() => {
    const later = setTimeout(() => flow.current?.fitView(FIT), 80)
    return () => clearTimeout(later)
  }, [fitKey])
  const [layout, setLayout] = useState<Layout | null>(null)
  // Laid out again only when the graph's shape changes, not on every refresh of the data.
  const shape = unit.id + '|' + unit.tasks.map(n => `${n.id}<${n.inputs.map(i => i.from + (i.primary ? '' : '~')).join(',')}`).join(';')
  const latest = useRef(unit)
  latest.current = unit
  useEffect(() => {
    let current = true
    layoutFlow(latest.current).then(l => current && setLayout(l))
    return () => { current = false }
  }, [shape])

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
    <div className="flow -mx-4" style={{ height: ready ? `min(${layout.h + 48}px, 70vh)` : 240 }}>
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
          fitView
          onInit={inst => { flow.current = inst }}
          fitViewOptions={FIT}
          minZoom={0.3}
          maxZoom={1.5}
          ariaLabelConfig={{
            'controls.ariaLabel': t('flow.controls'),
            'controls.zoomIn.ariaLabel': t('flow.zoomIn'),
            'controls.zoomOut.ariaLabel': t('flow.zoomOut'),
            'controls.fitView.ariaLabel': t('flow.fit'),
          }}
        >
          <Controls showInteractive={false} position="top-right" />
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
            <span className="block whitespace-pre-line wrap-anywhere">{n.text}</span>
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
          <span className={cn('truncate group-data-[state=open]:whitespace-normal', quiet && 'text-ink-2', f.state === 'rejected' && 'line-through')}>{f.problem}</span>
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
                <Tip key={id} text={nodeLabel(t, to)}>
                  <button type="button" className="chip" data-state={to.state} onClick={() => onPick(id)}>{to.taskId ? `#${to.taskId}` : to.title}</button>
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
      <CollapsibleContent className="pb-1.5 pl-6 text-xs">
        {suggests && <div><span className="text-ink-2">{t('io.field', { name: suggests })}</span>{f.recommendation?.reason}</div>}
        {f.reason && <div><span className="text-ink-2">{t('io.field', { name: t('finding.dropped') })}</span>{f.reason}</div>}
        {f.impact && <div><span className="text-ink-2">{t('io.field', { name: t('io.impact') })}</span>{f.impact}</div>}
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
          <span className="flex-none text-ink-2">{e.summary}</span>
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
    head = <><Icon name={glyph(n)} />{finderLabel(t, n)} · {t(n.result === 'running' ? 'state.running' : 'state.done')}</>
    input = <>{of && <div>{t('io.changesOf', { label: nodeLabel(t, of) })}</div>}{files.length ? paths(files) : quiet(t('io.noChanges'))}</>
    output = <>{found || quiet(t(n.result === 'running' ? 'io.noFindingsYet' : 'io.noFindings'))}<Checks n={n} /><Thumbs list={n.output.attachments ?? []} /></>
  } else if (n.kind === 'gate') {
    head = <><Icon name={glyph(n)} />{finderLabel(t, n)} · {t(`state.${n.state}`)}</>
    input = <><div>{n.input.text || n.title}</div><Steps steps={n.input.steps ?? []} /></>
    output = n.state === 'waiting'
      ? <GateForm key={gateKey(unit, n)} unit={unit} gate={n} />
      : <>
          {n.output.note && <div>{n.output.note}</div>}
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
      <div>{n.input.text || n.title}</div>
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
            options={unit.tasks.filter(x => x.taskId && x.kind !== 'gate').map(x => ({ value: x.id, label: `#${x.taskId} ${x.title || finderLabel(t, x)}` }))}
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
              <span className="min-w-0">{f.problem}</span>
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
                {c.live ? t('io.uncommitted') : c.subject}
                <span className="text-xs text-ink-2"> · {t('stat.files', { count: s.files })} +{num(s.add)} −{num(s.del)}{c.linked && ` · ${t('io.linked')}`}{c.rebasedFrom && ` · ${t('io.rebasedFrom', { hash: c.rebasedFrom })}`}</span>
              </div>
            )
          })
        : (n.lostCommits?.length ?? 0) > 0 ? quiet(t('io.lostCommits', { commits: n.lostCommits!.join(t('list.sep')) }))
        : n.state !== 'done' && <>{quiet(t('io.notStarted'))}{n.plans.length > 0 && <div className="font-mono text-xs text-ink-2">{t('io.willChange', { paths: n.plans.join(t('list.sep')) })}</div>}</>}
      {n.output.note && <div className="mt-1">{n.output.note}</div>}
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
        {/* The agent on work that is running or still to do, while its session lives: to its terminal. */}
        {n.agent && (n.state === 'running' || n.state === 'todo') && talk.sessions.some(s => s.agent === n.agent && !s.ended) && (
          <TerminalButton agent={n.agent} session={n.agentSession} className="ml-auto text-you-ink hover:bg-surface/60" />
        )}
      </div>
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
      </div>
    </div>
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

export function Activity({ unit, pick, onPick, talk, fitKey }: { unit: Unit; pick: Pick; onPick: (p: Pick) => void; talk: Talk; fitKey: unknown }) {
  const { t } = useT()
  const pickedId = pick?.kind === 'task' ? pick.id : null
  const picked = unit.tasks.find(n => n.id === pickedId)
  const pickNode = useMemo(() => (id: string | null) => onPick(id ? { kind: 'task', id } : null), [onPick])
  return (
    <section className="block-box">
      <h2 className="mb-2 text-lg font-[650]">{t('block.activity')}</h2>
      {unit.tasks.length ? (
        <>
          <Graph unit={unit} picked={pickedId} finding={pick?.finding} onPick={pickNode} talk={talk} fitKey={fitKey} />
          {picked && <Detail unit={unit} n={picked} finding={pick?.finding} onPick={onPick} talk={talk} />}
        </>
      ) : <p className="text-sm text-ink-2">{t('flow.empty')}</p>}
    </section>
  )
}
