// Layer 1, activity: the pieces of work handed to agents as a flow. Each is a node with
// something going in and something coming out; a line is one handing over to the next.
// It starts in one circle and has no end: a finished branch simply stops, in water colour.
import { applyNodeChanges, ControlButton, Controls, MiniMap, ReactFlow, type Edge, type Node, type NodeChange, type ReactFlowInstance } from '@xyflow/react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/ui/primitives/collapsible'
import { related } from './select'
import type { Pick } from './types'
import { useT } from '@/i18n'
import { growth } from './growth'
import type { Unit } from '@/planning/types'
import { turnsOf } from '@/conversations/select'
import { Questions } from '@/conversations/Turns'
import type { Talk } from '@/conversations/types'
import { Icon } from '@/ui/Icon'
import { W, H, type Layout, isStart, R, layoutFlow, type Line } from './layout'
import { type CardData, type LineData, type DotData, nodeTypes, edgeTypes } from './Nodes'
import { Detail } from './TaskDetail'
import { useResize } from '@/lib/useResize'
import { FLOW_HEIGHT } from '@/config/defaults'

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

function Graph({ unit, picked, finding, onPick, fitKey }: { unit: Unit; picked: string | null; finding?: string; onPick: (id: string | null) => void; fitKey: unknown }) {
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
  // The bottom edge drags the canvas's height, one for every unit; with none set (or after a
  // double-click) the canvas takes the graph's own height, as below.
  const height = useResize({ key: 'workriver.flow.height', variable: '--flow-h', ...FLOW_HEIGHT, max: () => window.innerHeight, edge: 'bottom' })
  // When the room around the graph changes (the rail folds, a panel's drag ends, the window
  // is resized, the canvas's height is dragged), the view settles again; and once more when the overview comes or goes, since
  // the canvas then takes the zoomed graph's height and room for the overview below it.
  useEffect(() => {
    let later = setTimeout(place, 80)
    const again = () => { clearTimeout(later); later = setTimeout(place, 150) }
    window.addEventListener('resize', again)
    return () => { clearTimeout(later); window.removeEventListener('resize', again) }
  }, [fitKey, height.size, place])
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
    const grew = growth(unit)
    // Work held up by an open blocker gate.
    const held = new Set(unit.tasks.filter(n => n.blocks && n.state !== 'done' && n.state !== 'dropped').map(n => n.blocks!))
    const at = (id: string) => layout.pos.get(id) ?? { x: 0, y: 0 }
    const fixed = { selectable: false, focusable: false }
    const nodes: Node[] = [
      { id: 'start', type: 'start', position: at('start'), width: R, height: R, data: { title: unit.purpose || t('flow.start') } satisfies DotData, ...fixed },
      // A node the layout has not placed yet (it just arrived) waits for the next layout.
      ...unit.tasks.filter(n => layout.pos.has(n.id)).map(n => ({
        id: n.id, type: n.kind, position: at(n.id), width: W, height: H, ...fixed,
        data: { node: n, dim: !isLit(n.id), picked: n.id === picked, onPick, talks: turnsOf(n).length, grew: grew.get(n.id), held: held.has(n.id) && n.state !== 'done' && n.state !== 'dropped' } satisfies CardData,
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
  }, [layout, unit, picked, finding, onPick, t])

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
    <div ref={box} className="flow relative -mx-4" style={{ height: `var(--flow-h, ${ready ? `min(${Math.round(mini ? layout.h * READABLE + 48 + MINI.height : layout.h + 48)}px, 70vh)` : '240px'})`, maxHeight: '100vh' }}>
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
      <div {...height.handle} aria-label={t('flow.resize')} className="edge" data-edge="bottom" />
    </div>
    {/* Below the canvas, never over it: no node can hide behind it. */}
    <Legend />
    </>
  )
}

const LEGEND = 'workriver.flow.legend'

/** What the graph's shapes, colours, marks and lines mean: one line ("Legend") until opened. */
function Legend() {
  const { t } = useT()
  const sw = (state: string, text: string) => <span className="key"><i className="sw" data-state={state} />{text}</span>
  const stroke = (state: string, text: string) => <span className="key"><i className="stroke" data-state={state} />{text}</span>
  const mark = (name: string, text: string) => <span className="key"><Icon name={name} />{text}</span>
  // Folded to its one line until asked for; the choice is each viewer's own.
  const [open, setOpen] = useState(() => { try { return localStorage.getItem(LEGEND) === '1' } catch { return false } })
  const toggle = (next: boolean) => { setOpen(next); try { localStorage.setItem(LEGEND, next ? '1' : '0') } catch { /* not remembered, still shown */ } }
  return (
    <Collapsible open={open} onOpenChange={toggle} className="mt-3 text-xs text-ink-2">
      <CollapsibleTrigger className="group/legend flex items-center gap-0.5 rounded-sm font-semibold hover:text-ink">
        <Icon name="chev" className="size-3 transition-transform group-data-[state=open]/legend:rotate-90" />{t('legend.title')}
      </CollapsibleTrigger>
      <CollapsibleContent className="flex flex-wrap gap-x-4 gap-y-1 pt-1">
      {sw('done', t('legend.done'))}{sw('running', t('legend.running'))}{sw('', t('legend.todo'))}<span className="key"><i className="sw" data-state="review" /><Icon name="code-review" />{t('kind.review')}</span>{sw('gate', t('kind.gate'))}<span className="key"><i className="count" aria-hidden />{t('legend.waiting')}</span><span className="key"><i className="grew" aria-hidden><Icon name="grew" />+</i>{t('legend.grew')}</span><span className="key"><i className="blocker" aria-hidden><Icon name="blocker" /></i>{t('flow.blocker')}</span>
      {mark('test', t('reviewer.test'))}
      {stroke('flowed', t('legend.flowed'))}{stroke('flowing', t('legend.flowing'))}{stroke('also', t('legend.also'))}
      </CollapsibleContent>
    </Collapsible>
  )
}

/** `title` and `hint`: another heading than "Activity", with a quiet line under it.
 *  `workspace`: the workspace of the unit's intent (a `Space.id`), when known. */
export function Activity({ unit, pick, onPick, talk, fitKey, title, hint, workspace }: { unit: Unit; pick: Pick; onPick: (p: Pick) => void; talk: Talk; fitKey: unknown; title?: string; hint?: string; workspace?: string }) {
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
          <Graph unit={unit} picked={pickedId} finding={pick?.finding} onPick={pickNode} fitKey={fitKey} />
          {picked && <Detail unit={unit} n={picked} finding={pick?.finding} onPick={onPick} talk={talk} workspace={workspace} />}
          {/* Past questions about the unit as a whole, with no task picked. */}
          {!picked && (unit.threads?.length ?? 0) > 0 && (
            <div className="mt-2"><Questions id={`questions-${unit.id}`} key={unit.id} threads={unit.threads!} agent={null} onFile={talk.onFile} /></div>
          )}
        </>
      ) : <p className="text-sm text-ink-2">{t('flow.empty')}</p>}
    </section>
  )
}
