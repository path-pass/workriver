import paths from './page-load-paths.txt?raw'

const routes = paths.trim().split('\n')
const literal = new Set(routes)
const templates = routes.filter(p => p.includes('{id}')).map(p => [p, new RegExp(`^${p.replace('{id}', '[^/]+')}$`)] as const)
const MAX_REQUESTS = 1024

type RequestTiming = { url: string; path: string; start: number; end?: number }
type RouteTiming = { path: string; count: number; ms: number; max_ms: number }
let initial: RequestTiming[] | null = []

function apiUrl(value: string) {
  try {
    const url = new URL(value, location.href)
    url.hash = ''
    return url
  } catch { return undefined }
}

function routeOf(url: URL) {
  if (url.origin !== location.origin) return
  return literal.has(url.pathname) ? url.pathname : templates.find(([, pattern]) => pattern.test(url.pathname))?.[0]
}

export function beginInitialRequest(value: string) {
  if (!initial || initial.length >= MAX_REQUESTS) return
  const url = apiUrl(value)
  if (!url) return
  const path = routeOf(url)
  if (!path) return
  const entry: RequestTiming = { url: url.href, path, start: performance.now() }
  initial.push(entry)
  return () => { entry.end ??= performance.now() }
}

export function finishInitialRequests(paint: number) {
  const requests = initial ?? []
  initial = null
  const measured = new Map<RequestTiming, PerformanceResourceTiming>()
  for (const resource of performance.getEntriesByType('resource') as PerformanceResourceTiming[]) {
    if (resource.startTime > paint || resource.responseEnd > paint) continue
    const request = requests.findLast(r => r.url === resource.name && r.start <= resource.startTime && !measured.has(r))
    if (request) {
      measured.set(request, resource)
    } else if (resource.initiatorType === 'img' && requests.length < MAX_REQUESTS) {
      const url = apiUrl(resource.name)
      const path = url && routeOf(url)
      if (path) requests.push({ url: resource.name, path, start: resource.startTime, end: resource.responseEnd })
    }
  }
  const grouped = new Map<string, RouteTiming>()
  for (const request of requests) {
    if (request.start > paint) continue
    const resource = measured.get(request)
    const elapsed = Math.round(Math.min(paint, resource?.responseEnd ?? request.end ?? paint) - (resource?.startTime ?? request.start))
    const row = grouped.get(request.path) ?? { path: request.path, count: 0, ms: 0, max_ms: 0 }
    row.count++
    row.ms += elapsed
    row.max_ms = Math.max(row.max_ms, elapsed)
    grouped.set(request.path, row)
  }
  return [...grouped.values()].sort((a, b) => a.path.localeCompare(b.path))
}
