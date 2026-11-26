import paths from './page-load-paths.txt?raw'

const routes = paths.trim().split('\n')
const literal = new Set(routes)
const templates = routes.filter(p => p.includes('{id}')).map(p => [p, new RegExp(`^${p.replace('{id}', '[^/]+')}$`)] as const)
const MAX_REQUESTS = 1024

type RouteTiming = { path: string; count: number; ms: number; max_ms: number }

function routeOf(url: URL) {
  if (url.origin !== location.origin) return
  return literal.has(url.pathname) ? url.pathname : templates.find(([, pattern]) => pattern.test(url.pathname))?.[0]
}

export function initialRequestTimings(paint: number) {
  const grouped = new Map<string, RouteTiming>()
  let count = 0
  for (const resource of performance.getEntriesByType('resource') as PerformanceResourceTiming[]) {
    if (resource.startTime >= paint || (resource.initiatorType !== 'fetch' && resource.initiatorType !== 'img')) continue
    const path = routeOf(new URL(resource.name))
    if (!path) continue
    const elapsed = Math.round(Math.min(paint, resource.responseEnd) - resource.startTime)
    const row = grouped.get(path) ?? { path, count: 0, ms: 0, max_ms: 0 }
    row.count++
    row.ms += elapsed
    row.max_ms = Math.max(row.max_ms, elapsed)
    grouped.set(path, row)
    if (++count === MAX_REQUESTS) break
  }
  return [...grouped.values()].sort((a, b) => a.path.localeCompare(b.path))
}

/** Diagnostic reports go to the service silently, including on a 401. */
export function postPageLoad(report: unknown) {
  void fetch('/api/perf/page-load', {
    method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(report),
  }).catch(() => {})
}
