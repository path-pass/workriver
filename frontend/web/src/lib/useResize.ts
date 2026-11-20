// A draggable edge that sets a panel's width. The width lives in a CSS variable while the
// pointer moves, so nothing re-renders per frame; it becomes state (and is remembered) when
// the drag ends, which is when anything that depends on it (a graph that refits) hears of it.
//
// Hand-written instead of react-resizable-panels: that library cost +15.3 kB gzip for this
// one handle (see docs/design/ui/navigation.md, the rail).
import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react'

interface Options {
  /** The localStorage key the width is kept under. */
  key: string
  /** The CSS variable (on the document) that carries the width, e.g. `--rail-w`. */
  variable: string
  def: number
  min: number
  max: number
  /** Which edge of the panel the handle is on: dragging away from the panel widens it. */
  edge: 'right' | 'left'
  /** One step with the arrow keys. */
  step?: number
}

const recall = (key: string) => { try { return Number(localStorage.getItem(key)) || 0 } catch { return 0 } }
const remember = (key: string, w: number) => { try { localStorage.setItem(key, String(w)) } catch { /* not remembered, still applied */ } }

export function useResize({ key, variable, def, min, max, edge, step = 16 }: Options) {
  const clamp = useCallback((w: number) => Math.round(Math.min(max, Math.max(min, w))), [min, max])
  const [width, setWidth] = useState(() => { const w = recall(key); return w ? clamp(w) : def })
  // Where a drag started: the pointer's x and the width then.
  const drag = useRef<{ x: number; w: number; now: number } | null>(null)
  const apply = useCallback((w: number) => document.documentElement.style.setProperty(variable, `${w}px`), [variable])
  useEffect(() => apply(width), [apply, width])
  const commit = (w: number) => { const c = clamp(w); apply(c); setWidth(c); remember(key, c) }
  const sign = edge === 'right' ? 1 : -1

  const onPointerDown = (e: PointerEvent<HTMLElement>) => {
    if (e.button !== 0) return
    e.preventDefault()
    e.currentTarget.setPointerCapture(e.pointerId)
    e.currentTarget.dataset.dragging = ''
    drag.current = { x: e.clientX, w: width, now: width }
    // Nothing gets selected while the edge is dragged across text.
    document.body.style.userSelect = 'none'
  }
  const onPointerMove = (e: PointerEvent<HTMLElement>) => {
    const d = drag.current
    if (!d) return
    d.now = clamp(d.w + sign * (e.clientX - d.x))
    apply(d.now)
    e.currentTarget.setAttribute('aria-valuenow', String(d.now))
  }
  const end = (e: PointerEvent<HTMLElement>) => {
    const d = drag.current
    if (!d) return
    drag.current = null
    delete e.currentTarget.dataset.dragging
    document.body.style.userSelect = ''
    commit(d.now)
  }
  const onKeyDown = (e: KeyboardEvent<HTMLElement>) => {
    const to = e.key === 'ArrowRight' ? width + sign * step : e.key === 'ArrowLeft' ? width - sign * step : e.key === 'Home' ? min : e.key === 'End' ? max : null
    if (to === null) return
    e.preventDefault()
    commit(to)
  }
  return {
    /** The width as last settled (not while dragging). */
    width,
    /** For the handle element; give it an `aria-label` and a class. */
    handle: {
      role: 'separator' as const,
      tabIndex: 0,
      'aria-orientation': 'vertical' as const,
      'aria-valuenow': width,
      'aria-valuemin': min,
      'aria-valuemax': max,
      onPointerDown,
      onPointerMove,
      onPointerUp: end,
      onPointerCancel: end,
      onDoubleClick: () => commit(def),
      onKeyDown,
    },
  }
}
