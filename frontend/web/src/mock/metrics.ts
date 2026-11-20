// Sample machines for VITE_MOCK=1: this machine (a Mac, memory pressure warning) with its
// CPU in alert and its compressor past the warning, a Linux box with PSI whose `/` is
// nearly full and that just had an OOM kill (both critical), a Linux box without PSI (an
// older kernel or a container), and a Mac that cannot be reached. Content, not UI text.
// Built relative to now, so the ages read the same whenever the sample is opened.
import type { MachineAlert, MetricDetail, MetricSample, MetricsReport, Proc, Thresholds } from '@/machines/types'

const STEP = 5
const GB = 1024 ** 3

const NONE: MetricDetail = { os: null, memCompressed: null, compressor: null, systemFree: null, pressure: null, pressureSome: null, pressureFull: null, oomKills: null, oomKillsWatched: null, swapUsed: null, swapTotal: null, disks: [] }

const mac = (pressure: MetricDetail['pressure'], compressed: number, compressor: number, swapUsed: number, free: number): MetricDetail =>
  ({ ...NONE, os: 'darwin', pressure, memCompressed: compressed * GB, compressor, systemFree: 31, swapUsed: swapUsed * GB, swapTotal: 4 * GB, disks: [{ mount: '/System/Volumes/Data', free: free * GB, total: 460 * GB }] })

/** `n` samples ending `end` ms before now, CPU and memory (percent) from the given curves. */
function series(n: number, end: number, cpu: (k: number) => number, mem: (k: number) => number, total: number, detail: MetricDetail): MetricSample[] {
  const now = Date.now()
  return Array.from({ length: n }, (_, k) => ({
    ...detail,
    at: now - end - (n - 1 - k) * STEP * 1000,
    cpu: Math.round(cpu(k) * 10) / 10,
    memUsed: Math.round((mem(k) / 100) * total),
    memTotal: total,
  }))
}

// A small wobble that is the same every time.
const wobble = (k: number, by: number) => Math.sin(k * 1.7) * by + Math.sin(k * 0.45) * by * 0.6

let thresholds: Thresholds = {
  cpu: { on: true, percent: 90, minutes: 5 },
  memory: { on: true, percent: 90, minutes: 5 },
  compressor: { on: true, warn: 60, crit: 80 },
  swap: { on: true, warn: 4, crit: 10, minutes: 1 },
  pressure: { on: true, minutes: 1 },
  systemFree: { on: true, warn: 25, crit: 20, minutes: 1 },
  available: { on: true, warn: 25, crit: 12, minutes: 1 },
  psi: { on: true, warn: 5, crit: 20, minutes: 1 },
  oom: { on: true, minutes: 5 },
  disk: { on: true, warn: 80, crit: 90, warnFree: 50, critFree: 20, minutes: 1 },
}

/** The compressor of this machine rose past 60 % two minutes ago; box's kill was a minute ago. */
const COMPRESSOR_FOR = 2 * 60 * 1000
const OOM_FOR = 60 * 1000

/** The CPU of this machine went over 90 % six minutes ago and stayed there. */
const HOT_FOR = 6 * 60 * 1000

export function mockMetrics(): MetricsReport {
  const now = Date.now()
  const hotFrom = 120 - HOT_FOR / (STEP * 1000)
  return {
    thresholds,
    stepSeconds: STEP,
    machines: [
      {
        label: 'local', here: true, reachable: true,
        samples: series(120, 0, k => (k < hotFrom ? 55 + wobble(k, 8) + k * 0.2 : 96 + wobble(k, 1.5)), k => 79 + wobble(k, 0.6), 32 * GB, mac('warning', 13.2, 66.4, 3.6, 80.6)),
        alerts: { cpu: { since: new Date(now - HOT_FOR).toISOString(), level: 'warn' }, compressor: { since: new Date(now - COMPRESSOR_FOR).toISOString(), level: 'warn' } },
      },
      {
        label: 'box', here: false, reachable: true,
        samples: series(120, 0, k => 31 + wobble(k, 9), k => 46 + wobble(k, 0.8), 64 * GB, {
          ...NONE, os: 'linux', pressureSome: 0.59, pressureFull: 0.12, oomKills: 4, oomKillsWatched: 1, swapUsed: 0.3 * GB, swapTotal: 8 * GB,
          disks: [{ mount: '/home', free: 312 * GB, total: 1800 * GB }, { mount: '/', free: 14 * GB, total: 98 * GB }],
        }),
        alerts: { oom: { since: new Date(now - OOM_FOR).toISOString(), level: 'crit' }, disk: { since: new Date(now - 40 * 60 * 1000).toISOString(), level: 'crit' } },
      },
      {
        label: 'ci', here: false, reachable: true,
        samples: series(120, 0, k => 12 + wobble(k, 4), k => 22 + wobble(k, 0.4), 16 * GB, {
          ...NONE, os: 'linux', swapUsed: 0, swapTotal: 0, disks: [{ mount: '/', free: 1.9 * 1024 * GB, total: 3.6 * 1024 * GB }],
        }),
        alerts: {},
      },
      {
        label: 'laptop', here: false, reachable: false, downSince: new Date(now - 7 * 60 * 1000).toISOString(),
        samples: series(36, 7 * 60 * 1000, k => 22 + wobble(k, 6), k => 57 + wobble(k, 0.5), 16 * GB, mac('normal', 1.1, 8, 0, 210)),
        alerts: {},
      },
    ],
  }
}

/**
 * A machine's minutes over the last `hours`: a working day's rise and fall, from the newest
 * sample's detail. The laptop's stop where it went away; "ci" was followed six hours ago.
 */
export function mockHistory(machine: string, hours: number): MetricSample[] {
  const m = mockMetrics().machines.find(x => x.label === machine)
  const last = m?.samples.at(-1)
  if (!m || !last) return []
  const now = Date.now()
  const end = m.reachable ? now : now - 7 * 60 * 1000
  const n = Math.min(hours, machine === 'ci' ? 6 : 24) * 60
  const day = (k: number) => Math.max(0, Math.sin(((k - n) / 1440) * 2 * Math.PI + 1.2))
  const memShare = last.memUsed / last.memTotal
  return Array.from({ length: n }, (_, k) => ({
    ...last,
    at: Math.floor((end - (n - k) * 60_000) / 60_000) * 60_000,
    cpu: Math.round(Math.min(99, (last.cpu ?? 30) * (0.3 + 0.7 * day(k)) + wobble(k / 20, 4) + 5) * 10) / 10,
    memUsed: Math.round(last.memTotal * Math.min(0.98, memShare * (0.8 + 0.2 * day(k)) + wobble(k / 20, 0.01))),
  })).filter((_, k) => machine !== 'box' || k < n - 300 || k > n - 240) // box: an hour away
}

/** The processes using the most memory, as an alert names them. */
const TOP: Proc[] = [
  { pid: 6567, name: 'claude', rss: 6.1 * GB, agent: 'reviewer', place: 'chat' },
  { pid: 57934, name: 'Google Chrome Helper (Renderer)', rss: 2.3 * GB },
  { pid: 7120, name: 'node', rss: 1.8 * GB, agent: 'claude 2', place: 'chat' },
  { pid: 1354, name: 'Google Chrome', rss: 0.5 * GB },
  { pid: 1431, name: 'Spotlight', rss: 0.4 * GB },
]

/** The alerts of the sample, as the workspace snapshot carries them. */
export function mockAlerts(): MachineAlert[] {
  const [here, box] = mockMetrics().machines
  const last = here.samples.at(-1)!
  const t = thresholds
  return [
    { machine: here.label, here: true, metric: 'cpu', level: 'warn', since: here.alerts.cpu!.since, value: last.cpu!, threshold: t.cpu.percent, minutes: t.cpu.minutes, top: TOP },
    { machine: here.label, here: true, metric: 'compressor', level: 'warn', since: here.alerts.compressor!.since, value: last.compressor!, threshold: t.compressor.warn, minutes: null, top: TOP },
    { machine: box.label, here: false, metric: 'oom', level: 'crit', since: box.alerts.oom!.since, value: 1, threshold: null, minutes: t.oom.minutes, top: [{ pid: 4242, name: 'python3', rss: 11.6 * GB, agent: 'tests', place: 'infra' }] },
    { machine: box.label, here: false, metric: 'disk', level: 'crit', since: box.alerts.disk!.since, value: 85.7, threshold: t.disk.crit, thresholdFree: t.disk.critFree, disk: { mount: '/', free: 14 * GB, total: 98 * GB }, minutes: t.disk.minutes, top: [] },
  ]
}

/** Settings in the sample: kept for the page's lifetime. */
export function setMockThresholds(t: Thresholds) {
  thresholds = t
}
