// Sample machines for VITE_MOCK=1: this machine with its CPU in alert, a followed machine
// at ease, and one that cannot be reached. Content, not UI text. Built relative to now, so
// the ages read the same whenever the sample is opened.
import type { MachineAlert, MetricSample, MetricsReport, Thresholds } from '@/types'

const STEP = 5
const GB = 1024 ** 3

/** `n` samples ending `end` ms before now, CPU and memory (percent) from the given curves. */
function series(n: number, end: number, cpu: (k: number) => number, mem: (k: number) => number, total: number): MetricSample[] {
  const now = Date.now()
  return Array.from({ length: n }, (_, k) => ({
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
        samples: series(120, 0, k => (k < hotFrom ? 55 + wobble(k, 8) + k * 0.2 : 96 + wobble(k, 1.5)), k => 79 + wobble(k, 0.6), 32 * GB),
        alerts: { cpu: new Date(now - HOT_FOR).toISOString() },
      },
      {
        label: 'box', here: false, reachable: true,
        samples: series(120, 0, k => 31 + wobble(k, 9), k => 46 + wobble(k, 0.8), 64 * GB),
        alerts: {},
      },
      {
        label: 'laptop', here: false, reachable: false, downSince: new Date(now - 7 * 60 * 1000).toISOString(),
        samples: series(36, 7 * 60 * 1000, k => 22 + wobble(k, 6), k => 57 + wobble(k, 0.5), 16 * GB),
        alerts: {},
      },
    ],
  }
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
