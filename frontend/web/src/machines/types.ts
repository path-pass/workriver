// The machines workriver follows, and what they say about themselves.
/** What a machine alerts on: macOS has compressor, swap, pressure and systemFree; Linux
 * available, psi and oom; every machine cpu, memory and disk. */
export type Metric = 'cpu' | 'memory' | 'compressor' | 'swap' | 'pressure' | 'systemFree' | 'available' | 'psi' | 'oom' | 'disk';

/** How bad an alert is. */
export type Level = 'warn' | 'crit';

/** One of the processes using the most memory when an alert rose. */
export interface Proc {
  pid: number;
  name: string;
  /** Resident memory, bytes. */
  rss: number;
  /** The agent it descends from, and where that agent runs (its workspace). */
  agent?: string;
  place?: string;
}

/** A machine past an alert threshold, for Needs you. */
export interface MachineAlert {
  /** `local` for the machine workriver runs on (`here`). */
  machine: string;
  here: boolean;
  metric: Metric;
  level: Level;
  /** Since when it has been past the threshold without a break. */
  since: string;
  /** The newest value in the metric's unit: percent; GB for swap; kills for oom; the
   * fullest filesystem's used percent for disk; 0 for pressure (its level says it). */
  value: number;
  /** The threshold `level` is past, in the same unit; null for pressure and oom. */
  threshold: number | null;
  /** disk: the free GB of `level`, and the filesystem nearest its limits. */
  thresholdFree?: number;
  disk?: Disk;
  /** How long it had to hold; null: on the sample that saw it. oom: how long it is held. */
  minutes: number | null;
  /** Largest first; empty until read. */
  top: Proc[];
}

/** CPU and memory: at or above `percent` for `minutes` without a break (one level, warn). */
export interface Limit {
  on: boolean;
  percent: number;
  minutes: number;
}

/** Two levels of a value, held for `minutes` (absent: at once). */
export interface Levels {
  on: boolean;
  warn: number;
  crit: number;
  minutes?: number;
}

/** Fixed levels: pressure (held `minutes`), oom (critical at once, held `minutes`). */
export interface Timed {
  on: boolean;
  minutes: number;
}

/** A filesystem: used percent over warn / crit, or free GB under warnFree / critFree. */
export interface DiskLimit {
  on: boolean;
  warn: number;
  crit: number;
  warnFree: number;
  critFree: number;
  minutes: number;
}

/** The same for every machine; each applies where the machine's system has the value. */
export interface Thresholds {
  cpu: Limit;
  memory: Limit;
  compressor: Levels;
  swap: Levels;
  pressure: Timed;
  systemFree: Levels;
  available: Levels;
  psi: Levels;
  oom: Timed;
  disk: DiskLimit;
}

export type Pressure = 'normal' | 'warning' | 'critical'

/** One filesystem's room, in bytes. */
export interface Disk {
  mount: string;
  free: number;
  total: number;
}

/** What a machine says beyond CPU and memory; null where its system does not say it. */
export interface MetricDetail {
  os: 'linux' | 'darwin' | null;
  /** macOS: memory the compressor occupies. */
  memCompressed: number | null;
  /** macOS: how full the compressor is, percent of its limit (at 100 the kernel is out of it). */
  compressor: number | null;
  /** macOS: the share of memory the kernel counts as free, percent. */
  systemFree: number | null;
  /** macOS: the kernel's memory pressure level. */
  pressure: Pressure | null;
  /** Linux PSI, percent of the last 10 s some / all tasks waited for memory; null without PSI. */
  pressureSome: number | null;
  pressureFull: number | null;
  /** Linux: OOM kills since boot, and since workriver began to read the machine. */
  oomKills: number | null;
  oomKillsWatched: number | null;
  swapUsed: number | null;
  swapTotal: number | null;
  /** The home folder's filesystem first, then `/` when it is another one. */
  disks: Disk[];
}

export interface MetricSample extends MetricDetail {
  /** Milliseconds since 1970. */
  at: number;
  /** Percent; null until two readings of counters are there (Linux). */
  cpu: number | null;
  /** Total − available, the same on every machine. */
  memUsed: number;
  memTotal: number;
}

/** One machine's CPU and memory over the last minutes. */
export interface MachineMetrics {
  label: string;
  here: boolean;
  /** Its newest probe answered. */
  reachable: boolean;
  downSince?: string;
  /** Why the newest probe gave nothing, when it ran but failed. */
  error?: string;
  /** Oldest first. */
  samples: MetricSample[];
  /** The metrics in alert: since when, and how bad. */
  alerts: Partial<Record<Metric, { since: string; level: Level }>>;
}

export interface MetricsReport {
  thresholds: Thresholds;
  stepSeconds: number;
  /** This machine first, then the followed ones. */
  machines: MachineMetrics[];
}

/** Another machine whose agents are followed over ssh. */
export interface Machine {
  label: string;
  ssh: string;
  enabled: boolean;
  /** The port over there that leads back to the service, while connected (a new one per
   *  connection, written there to ~/.config/workriver/port). */
  port?: number | null;
  /** The connection is down: since when. */
  downSince?: string;
  /** The next attempt to connect, in seconds. */
  retryIn?: number;
  /** Its agents can be read now. */
  reachable: boolean;
  /** Just enabled: the first attempt to connect has not ended yet. */
  connecting?: boolean;
  error?: string;
  /** The connection is up. */
  tunnel: boolean;
  tunnelError?: string;
  agents: number;
}
