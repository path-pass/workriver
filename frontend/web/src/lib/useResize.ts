// A draggable edge that sets a panel's width, or its height when the edge is at the bottom.
// The size lives in a CSS variable while the pointer moves, so nothing re-renders per frame;
// it becomes state (and is remembered) when the drag ends, which is when anything that
// depends on it (a graph that refits) hears of it.
//
// Hand-written instead of react-resizable-panels: that library cost +15.3 kB gzip for this
// one handle (see docs/design/ui/navigation.md, the rail).
import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react'

interface Options {
  /** The localStorage key the size is kept under. */
  key: string
  /** The CSS variable (on the document) that carries the size, e.g. `--rail-w`. */
  variable: string
  /** The size with none set. Without one the panel sizes itself (the variable is unset, so
   *  its own fallback applies) until the edge is moved, and double-click goes back to that. */
  def?: number
  min: number
  /** The most; a function when it moves without a render (the window's height), read each
   *  time the size is bounded. */
  max: number | (() => number)
  /** Which edge of the panel the handle is on: dragging away from the panel enlarges it. */
  edge: 'right' | 'left' | 'bottom'
  /** One step with the arrow keys. */
  step?: number
}

const recall = (key: string) => { try { return Number(localStorage.getItem(key)) || 0 } catch { return 0 } }
const remember = (key: string, w: number) => { try { if (w) localStorage.setItem(key, String(w)); else localStorage.removeItem(key) } catch { /* not remembered, still applied */ } }

export function useResize({ key, variable, def, min, max, edge, step = 16 }: Options) {
  const most = typeof max === 'function' ? max : () => max
  const clamp = (w: number) => Math.round(Math.min(most(), Math.max(min, w)))
  // 0: none set, the panel sizes itself.
  const [size, setSize] = useState(() => { const w = recall(key); return w ? clamp(w) : def ?? 0 })
  // Where a drag started: the pointer's position and the size then; whether it has moved.
  const drag = useRef<{ at: number; w: number; now: number; moved: boolean } | null>(null)
  const apply = useCallback((w: number) => {
    const s = document.documentElement.style
    if (w) s.setProperty(variable, `${w}px`); else s.removeProperty(variable)
  }, [variable])
  useEffect(() => apply(size), [apply, size])
  const commit = (w: number) => { const c = w ? clamp(w) : 0; apply(c); setSize(c); remember(key, c) }
  const down = edge === 'bottom'
  const sign = edge === 'left' ? -1 : 1
  const along = (e: { clientX: number; clientY: number }) => (down ? e.clientY : e.clientX)
  // The size to start from: what the panel (the handle's parent) has now, which is the one set
  // unless CSS caps it (a window grown smaller), else the one set.
  const from = (handle: HTMLElement) => (down ? handle.parentElement?.offsetHeight : handle.parentElement?.offsetWidth) || size || min

  const onPointerDown = (e: PointerEvent<HTMLElement>) => {
    if (e.button !== 0) return
    e.preventDefault()
    e.currentTarget.setPointerCapture(e.pointerId)
    e.currentTarget.dataset.dragging = ''
    const w = from(e.currentTarget)
    drag.current = { at: along(e), w, now: w, moved: false }
    // Nothing gets selected while the edge is dragged across text.
    document.body.style.userSelect = 'none'
  }
  const onPointerMove = (e: PointerEvent<HTMLElement>) => {
    const d = drag.current
    if (!d || (along(e) === d.at && !d.moved)) return
    d.moved = true
    d.now = clamp(d.w + sign * (along(e) - d.at))
    apply(d.now)
    e.currentTarget.setAttribute('aria-valuenow', String(d.now))
  }
  const end = (e: PointerEvent<HTMLElement>) => {
    const d = drag.current
    if (!d) return
    drag.current = null
    delete e.currentTarget.dataset.dragging
    document.body.style.userSelect = ''
    // A click that did not move changes nothing: an automatic size stays automatic.
    if (d.moved) commit(d.now)
  }
  const onKeyDown = (e: KeyboardEvent<HTMLElement>) => {
    const [more, less] = down ? ['ArrowDown', 'ArrowUp'] : ['ArrowRight', 'ArrowLeft']
    const w = from(e.currentTarget)
    const to = e.key === more ? w + sign * step : e.key === less ? w - sign * step : e.key === 'Home' ? min : e.key === 'End' ? most() : null
    if (to === null) return
    e.preventDefault()
    commit(to)
  }
  return {
    /** The size as last settled (not while dragging); 0 while the panel sizes itself. */
    size,
    /** For the handle element; give it an `aria-label` and a class. */
    handle: {
      role: 'separator' as const,
      tabIndex: 0,
      // A separator's orientation is the line it draws: a bottom edge is a horizontal one.
      'aria-orientation': down ? 'horizontal' as const : 'vertical' as const,
      'aria-valuenow': size || undefined,
      'aria-valuemin': min,
      'aria-valuemax': most(),
      onPointerDown,
      onPointerMove,
      onPointerUp: end,
      onPointerCancel: end,
      onDoubleClick: () => commit(def ?? 0),
      onKeyDown,
    },
  }
}
