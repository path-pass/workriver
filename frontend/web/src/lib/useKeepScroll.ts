import { useCallback, useRef } from 'react'

/** Whether the element scrolls up and down and has room to go further that way. */
function canScroll(el: HTMLElement, dy: number) {
  const y = getComputedStyle(el).overflowY
  if (y !== 'auto' && y !== 'scroll') return false
  return dy < 0 ? el.scrollTop > 0 : el.scrollTop + el.clientHeight < el.scrollHeight - 1
}

/**
 * A ref for a panel pinned to the window: scrolling that starts in it stays in it. What
 * nothing in the panel can scroll further is cancelled rather than passed to the page; on a
 * Mac it dragged the whole page, this panel with it, over empty space (the rubber band).
 * CSS (`overscroll-behavior`) only holds where something scrolls, not over a header or a box.
 * Sideways scrolling passes (swiping back and forward); so does all of it while the panel is
 * laid out as part of the page (narrow screens).
 */
export function useKeepScroll<T extends HTMLElement>() {
  const off = useRef<(() => void) | null>(null)
  // A callback ref: a panel swaps its element as it folds and opens.
  return useCallback((panel: T | null) => {
    off.current?.()
    off.current = null
    if (!panel) return
    const onWheel = (e: WheelEvent) => {
      if (Math.abs(e.deltaX) > Math.abs(e.deltaY) || getComputedStyle(panel).position === 'static') return
      for (let el = e.target instanceof HTMLElement ? e.target : null; el && el !== panel; el = el.parentElement)
        if (canScroll(el, e.deltaY)) return
      e.preventDefault()
    }
    panel.addEventListener('wheel', onWheel, { passive: false })
    off.current = () => panel.removeEventListener('wheel', onWheel)
  }, [])
}
