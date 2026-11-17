// Sample machines for VITE_MOCK=1: this machine (a Mac, memory pressure warning) with its
// CPU in alert, a Linux box at ease with PSI and a few OOM kills, a Linux box without PSI
// (an older kernel or a container), and a Mac that cannot be reached. Content, not UI text. Built relative to now, so
// the ages read the same whenever the sample is opened.
import type { MachineAlert, MetricDetail, MetricSample, MetricsReport, Thresholds } from '@/types'

const STEP = 5
const GB = 1024 ** 3

const NONE: MetricDetail = { os: null, memCompressed: null, pressure: null, pressureSome: null, pressureFull: null, oomKills: null, oomKillsWatched: null, swapUsed: null, swapTotal: null, disks: [] }

const mac = (pressure: MetricDetail['pressure'], compressed: number, swapUsed: number, free: number): MetricDetail =>
  ({ ...NONE, os: 'darwin', pressure, memCompressed: compressed * GB, swapUsed: swapUsed * GB, swapTotal: 4 * GB, disks: [{ mount: '/System/Volumes/Data', free: free * GB, total: 460 * GB }] })

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

let thresholds: Thresholds = { cpu: { on: true, percent: 90, minutes: 5 }, memory: { on: true, percent: 90, minutes: 5 } }

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
        samples: series(120, 0, k => (k < hotFrom ? 55 + wobble(k, 8) + k * 0.2 : 96 + wobble(k, 1.5)), k => 79 + wobble(k, 0.6), 32 * GB, mac('warning', 13.2, 3.6, 80.6)),
        alerts: { cpu: new Date(now - HOT_FOR).toISOString() },
      },
      {
        label: 'box', here: false, reachable: true,
        samples: series(120, 0, k => 31 + wobble(k, 9), k => 46 + wobble(k, 0.8), 64 * GB, {
          ...NONE, os: 'linux', pressureSome: 0.59, pressureFull: 0.12, oomKills: 4, oomKillsWatched: 1, swapUsed: 0.3 * GB, swapTotal: 8 * GB,
          disks: [{ mount: '/home', free: 312 * GB, total: 1800 * GB }, { mount: '/', free: 41 * GB, total: 98 * GB }],
        }),
        alerts: {},
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
        samples: series(36, 7 * 60 * 1000, k => 22 + wobble(k, 6), k => 57 + wobble(k, 0.5), 16 * GB, mac('normal', 1.1, 0, 210)),
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

/** The alerts of the sample, as the workspace snapshot carries them. */
export function mockAlerts(): MachineAlert[] {
  const m = mockMetrics().machines[0]
  return [{ machine: m.label, here: true, metric: 'cpu', since: m.alerts.cpu!, value: m.samples.at(-1)!.cpu!, percent: thresholds.cpu.percent, minutes: thresholds.cpu.minutes }]
}

/** Settings in the sample: kept for the page's lifetime. */
export function setMockThresholds(t: Thresholds) {
  thresholds = t
}
