// Where a unit's tasks sit on the graph.
import { AntVDagreLayout } from '@antv/layout'
import type { Unit } from '@/planning/types'

export const W = 176, H = 48, R = 18

// ---- layout: a layered layout (dagre) places the nodes left to right once; React Flow
// draws the lines, so they follow a node when it is dragged ----

export interface Line { id: string; source: string; target: string; primary: boolean }
export interface Layout { unitId: string; h: number; pos: Map<string, { x: number; y: number }>; lines: Line[] }

export const isStart = (id: string) => id === 'start'

export async function layoutFlow(unit: Unit): Promise<Layout> {
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
