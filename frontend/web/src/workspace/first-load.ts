import { post } from '@/api/http'
import { finishInitialRequests } from '@/api/initial-requests'
import type { Workspace } from './types'

declare const __WORKRIVER_BUILD__: number

export const WORKSPACE_PATH = '/api/workspace'

type WorkspaceTiming = {
  request_ms?: number
  response_ms?: number
  bytes?: number
  ready_ms: number
  parse_ms?: number
}

const snapshots = new WeakMap<Workspace, WorkspaceTiming>()
let reported = false

const ms = (time: number | undefined) => time === undefined ? undefined : Math.round(time)
const resources = (url: string) => performance.getEntriesByName(url, 'resource') as PerformanceResourceTiming[]

export function recordWorkspaceReady(ws: Workspace, started: number) {
  if (reported) return
  const ready = Math.round(performance.now())
  const entry = resources(new URL(WORKSPACE_PATH, location.href).href).findLast(r => r.startTime >= started)
  const response = ms(entry?.responseEnd)
  snapshots.set(ws, {
    request_ms: ms(entry?.requestStart),
    response_ms: response,
    bytes: entry?.encodedBodySize,
    ready_ms: ready,
    parse_ms: response === undefined ? undefined : Math.max(0, ready - response),
  })
}

function downloaded(url?: string) {
  return url ? ms(resources(url)[0]?.responseEnd) : undefined
}

export function reportFirstBoardPaint(ws: Workspace) {
  const timing = snapshots.get(ws)
  if (!timing || reported) return
  const committed = Math.round(performance.now())
  let frame = requestAnimationFrame(function firstFrame() {
    if (reported) return
    if (document.hidden) {
      frame = requestAnimationFrame(firstFrame)
      return
    }
    reported = true
    // A task queued from the frame can run after the browser's rendering update.
    setTimeout(() => {
      const paint = performance.now()
      const js = document.querySelector<HTMLScriptElement>('script[type="module"][src]')?.src
      const css = document.querySelector<HTMLLinkElement>('link[rel="stylesheet"][href]')?.href
      const report = {
        ...timing,
        ms: Math.round(paint),
        js_ms: downloaded(js),
        css_ms: downloaded(css),
        app_ms: committed - timing.ready_ms,
        render_ms: Math.round(paint) - committed,
        git_pending: Number(!!ws.gitPending),
        build: __WORKRIVER_BUILD__,
        api: finishInitialRequests(paint),
      }
      void post('/api/perf/page-load', report, { notifySignedOut: false }).catch(() => {})
    }, 0)
  })
  return () => cancelAnimationFrame(frame)
}
