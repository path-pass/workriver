// The shape of what the UI shows: what the workriver service answers on /api/workspace,
// and what src/mock/demo.json holds for the sample.

export interface Workspace {
  /** Every repository a unit works in. */
  repos: Repo[];
  builtAt: string;
  intents: Intent[];
  /** One per agent. What it is working on comes from the tasks; only this does not. */
  sessions: Session[];
  /** Registered folders of repositories, and repositories outside all of them. Absent in the sample. */
  projects?: Project[];
  /** Deferred findings that came from outside workriver's tasks (an earlier review, a notes file). */
  backlog?: BacklogFinding[];
  /** Chores that belong to no intent: a project's (`project`), or nobody's. */
  chores?: Chore[];
  /** Every runtime workspace per machine, with the local one a remote one is linked to. */
  workspaces?: RuntimeWorkspace[];
  /** The other machines followed. */
  machines?: Machine[];
  /** Machines past an alert threshold now (the user's view only). */
  alerts?: MachineAlert[];
}

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

/** A runtime (Herdr) workspace, of this machine or of another. */
export interface RuntimeWorkspace {
  id: string;
  label: string;
  /** Absent: this machine. */
  machine?: string;
  /** A remote one: the label of the workspace of this machine it is one with. */
  linkedTo?: string;
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

/** A folder holding one or more repositories; or a repository outside every registered one. */
export interface Project {
  /** Its folder, which is what intents and backlog findings name. */
  id: string;
  title: string;
  registered: boolean;
  repos: string[];
}

/** Where an item is tracked elsewhere (Jira, Linear, GitHub). Only shown, never synced yet. */
export interface External {
  system: string;
  key: string;
  url?: string | null;
}

export interface BacklogFinding {
  /** The project's folder. */
  project: string;
  /** The intent it was batched into, or is meant for. */
  intent?: string;
  /** Where it came from, in words: which review, branch, date. */
  source: string;
  by: string;
  finding: Finding;
}

/**
 * planned  in its project's roadmap, not started
 * active   started in a runtime workspace, whose manager runs it
 * done / dropped
 */
export type IntentState = 'planned' | 'active' | 'done' | 'dropped';

/**
 * A goal in a project and the units that serve it. It runs in a runtime (Herdr) workspace
 * once its manager starts it; a workspace may run several.
 */
export type UnitProgress = 'working' | 'waiting' | 'ready' | 'merged' | 'done'

export interface Intent {
  id: string;
  /** Still active, with every unit merged or done: its manager can close it. */
  complete?: boolean;
  title: string;
  /** One sentence on what it is for, set by its manager or the user; may be empty or absent. */
  goal?: string;
  /** Absent in the sample: active. */
  state?: IntentState;
  /** Who decides its open findings: its manager (the default), or only the user. */
  findings?: 'user' | 'manager';
  /** Its project's folder (a `Project.id`); empty when nothing says. */
  project?: string;
  /** More about it; may hold links. */
  note?: string;
  external?: External;
  /** The runtime workspace it runs (or last ran) in. */
  workspace?: { id: string; label: string };
  /** Planned: its place in its project's roadmap, from 0. */
  order?: number;
  /** Dropped: why. */
  reason?: string;
  units: Unit[];
  /** Things agents left for the user that block nobody. Never on a graph. */
  chores: Chore[];
  /** The agent that splits the intent's work and hands it out; absent when none registered. */
  manager?: string;
  /** The manager's state in the runtime, or 'gone' when it is not running. */
  managerState?: RuntimeState | 'gone';
}

/** An agent's state as the runtime (Herdr) reports it. */
export type RuntimeState = 'working' | 'idle' | 'done' | 'blocked' | 'unknown';

export interface Repo {
  /** Where it is on disk; a unit names its repository by this. */
  path: string;
  /** The machine it is on; absent: this one. */
  machine?: string;
  name: string;
  /** The code as a tree of folders and files. The root has an empty path. */
  modules: ModuleNode;
}

export interface Session {
  /** The name shown; it may repeat across runtime workspaces. */
  agent: string;
  /** The session's own id, unique; absent in the sample. */
  session?: string;
  /** What the session is about (its terminal title), while it lives. */
  topic?: string;
  /** Waiting for a person in its terminal (a question, an approval) since then. */
  blockedSince?: string;
  /** The runtime workspace (in Herdr) it runs in; a remote one not linked names its machine. */
  group?: { id: string; label: string; machine?: string };
  /** The machine it runs on; absent: this one. */
  machine?: string;
  /** Its machine cannot be reached: shown as last seen. */
  unreachable?: boolean;
  /** The session is gone: the agent is not shown anywhere any more. */
  ended: boolean;
  /** While it lives: its state in the runtime. */
  state?: RuntimeState;
  /** Id of the current intent of the workspace it runs in. */
  intent?: string;
  /** It is the manager of its workspace. */
  manager?: boolean;
}

/**
 * Where the service's message about a user's input (a decision, a gate result) is: not
 * delivered (with why), delivered, or picked up (the receiver started working on it).
 */
export interface Sent {
  step: 'not-delivered' | 'delivered' | 'picked';
  /** The receiving agent. */
  to: string | null;
  toManager: boolean;
  /** Why it was not delivered; shown in the user's language. */
  reasonCode?: ReasonCode;
  /** The same in the service's words (for a runtime error, the runtime's own). */
  reason?: string;
  /** The message as sent. */
  message: string;
}

/** One thing to do by hand, with the command for it when there is one. */
export interface Step {
  text: string;
  command?: string;
}

export interface Chore {
  id: string;
  /** A chore of no intent: its project's folder ('' when it belongs to no project). */
  project?: string;
  title: string;
  /** The agent that asks for it. */
  by: string;
  steps: Step[];
  state: 'open' | 'done';
  /** When it was asked for. */
  since: string;
  /** Id of the unit it belongs to; absent when it concerns the intent as a whole. */
  unit?: string;
}

export interface ModuleNode {
  name: string;
  /** Repository-relative path; '' for the root. */
  path: string;
  /** Lines of code under this node. */
  loc: number;
  /** Number of source files under this node (1 for a file). */
  files: number;
  /** Present on folders (possibly empty), absent on files. */
  children?: ModuleNode[];
}

export type UnitStatus = 'active' | 'planned' | 'merged';

/** A piece of an intent that can be implemented on its own, on its own branch. */
export interface Unit {
  id: string;
  /** The intent's own work (research, design) shown as a unit with no repository or branch. */
  intentLevel?: boolean;
  title: string;
  /** One sentence on what the unit is for; may be empty. */
  purpose: string;
  /** Path of its repository, one of `Workspace.repos`. */
  repo: string;
  /** The machine its repository is on; absent: this one. */
  machine?: string;
  /** The branch the work happens on; null once merged. */
  branch: string | null;
  /** The branches its work was on before (a rebase onto a new branch, a rename). */
  previously?: string[];
  status: UnitStatus;
  /** Where it stands: a task to do (working), something waiting on a decision or the user,
   *  everything delivered and decided (ready to merge), merged, or closed by hand (done). */
  progress?: UnitProgress;
  /** Closed without a merge: why. */
  doneReason?: string;
  /** The work handed out, as a graph. Empty for units that predate recording. */
  tasks: Task[];
  /** Oldest first. The last one may be uncommitted work (`live`). */
  commits: Commit[];
  /** Which texts its files have: their texts are read again (`loadCode`) only when it changes. */
  code?: string;
}

/**
 * implement  work an agent does
 * review     a look at that work
 * gate       work only the user can do, which an agent cannot continue without
 */
export type TaskKind = 'implement' | 'review' | 'gate';

/** What a gate asks of the user. More kinds may follow. */
/** decide: the user discusses a design (asking why) and confirms it; the note is the decision. */
export type GateKind = 'deploy' | 'test' | 'decide';

/**
 * done         finished
 * running      an agent is on it now
 * todo         handed out, not started
 * waiting      gates only: the user has not done it yet
 * unregistered a commit that names no node: a gap in the record
 * dropped      no longer needed (kept, never deleted); nothing waits on it
 */
export type TaskState = 'done' | 'running' | 'todo' | 'waiting' | 'unregistered' | 'dropped';

export type Severity = 'P1' | 'P2' | 'P3';

/**
 * open: waiting for the user's decision. accepted: work was handed out for it. revised: to
 * be fixed the user's way (their note says how), work handed out like accepted.
 * rejected: decided against. deferred: put off, waiting in the project's backlog.
 * scheduled: picked with others to be done together. done: the work it caused was
 * delivered. dropped: taken out of the backlog; it will not be done.
 */
export type FindingState = 'open' | 'accepted' | 'revised' | 'rejected' | 'deferred' | 'scheduled' | 'done' | 'dropped';

/**
 * One problem a review, or the user at a test gate, found. A decision on it is not a node:
 * it stays inside the node that found it.
 */
export interface Finding {
  /** Unique within its node, shown to the user, e.g. 'P1-2'. */
  id: string;
  severity: Severity;
  problem: string;
  /** What happens if it is left; may be empty. */
  impact: string;
  /** File paths that show the problem. */
  evidence: string[];
  state: FindingState;
  /** Ids of the nodes handed out for it. Only an accepted finding has any; it may have several. */
  leadsTo: string[];
  /** Who made the decision; absent while the finding is open. An agent may decide on its own. */
  decidedBy?: 'agent' | 'user';
  /** While open: what the agent suggests, with one sentence of why. */
  recommendation?: { decision: 'accept' | 'reject' | 'defer'; reason: string };
  /** The user's one line with a decision. */
  note?: string;
  /** Pictures that show it (screenshots from a manual test). */
  attachments?: Attachment[];
  /** Where the message with the user's decision is. */
  delivery?: Sent;
  external?: External;
  /** Since when it is known: its task's delivery, or when it was added to the backlog. */
  since?: string;
  /** Dropped: why. */
  reason?: string;
  /** While open: 'manager' when it is the intent's manager's to decide (and one is running). */
  decider?: 'manager';
  /** While open: it would be the manager's, but none is registered or running: it falls to the user. */
  noManager?: boolean;
  /** What was added to it since it was reported, oldest first. */
  notes?: { by: string; at: string; text: string; attachments?: Attachment[] }[];
}

/** A check run on a piece of work: the command and its result, e.g. "12 passed". */
export interface Evidence {
  command: string;
  result: 'passed' | 'failed';
  summary: string;
  /** A screenshot or report of the check. */
  attachment?: Attachment;
}

/** A stored file (a screenshot, a report, a log), or a picture the user is attaching. */
export interface Attachment {
  id: string;
  name: string;
  /** Where the page shows it from: the service's store, or an object URL before upload. */
  url: string;
  /** Its type: pictures, Markdown and plain text preview, anything else downloads. Absent means a picture. */
  type?: string;
}

export type Reviewer = 'security-review' | 'code-review' | 'test';

export interface TaskInput {
  /** Id of the node this one follows, or 'start' for work cut straight from the unit. */
  from: string;
  /** The direct cause. Others (primary: false) are merely involved; drawn dashed. */
  primary: boolean;
}

/** One piece of work handed to one agent (implement, review) or to the user (gate). */
export interface Task {
  id: string;
  kind: TaskKind;
  state: TaskState;
  /** The task number shown as #n; null for reviews and unregistered commits. */
  taskId: string | null;
  /** What the work is; empty for reviews (their label is built from reviewer and result). */
  title: string;
  agent: string | null;
  /** The agent's session, to tell agents of the same name apart; absent in the sample. */
  agentSession?: string | null;
  /** True while the agent has uncommitted changes in its working directory. */
  writing: boolean;
  /** Who raised an implement node. 'plan' means the original breakdown. */
  raisedBy?: Reviewer | 'user' | 'plan';
  /** Gate nodes only. */
  gate?: GateKind;
  /**
   * When the node came to wait for the user: a gate became waiting, a review or gate with
   * open findings finished. Absent when nothing waits.
   */
  since?: string;
  /** Questions asked about this work and their answers, oldest first (the sample's form). */
  thread?: Turn[];
  /** Conversations about this work, oldest first (from the service). */
  threads?: Thread[];
  /** A design task: implement work whose output is its design note. */
  design?: boolean;
  /** A research task: implement work whose output is its note and attached report. */
  research?: boolean;
  /** Done without seeing other work (notes, evidence, other findings): an independent view. */
  blind?: boolean;
  /** It has no commits on the unit's branch now, but had these on a branch the unit left. */
  lostCommits?: string[];
  /** Waiting gates: the latest conclusion about it or the work it follows, to start the note from. */
  draft?: string;
  /**
   * Running, and its agent stopped without delivering: idle since `since` (or gone), when it
   * was reminded, and whether that did not help (then it is the user's and the manager's).
   */
  stalled?: { since: string; gone?: boolean; reminded?: string; escalated?: boolean };
  /** Review nodes only. */
  reviewer?: Reviewer;
  /** Review nodes only: found problems, found none, or still running. */
  result?: 'found' | 'passed' | 'running';
  inputs: TaskInput[];
  input: {
    text: string;
    /**
     * Which findings of which review (or gate) nodes the work answers. Empty for work cut
     * straight from the unit or asked for by the user.
     */
    causes: { review: string; findings: string[] }[];
    /** Gate nodes only: what the agent prepared for the user to do. */
    steps?: Step[];
  };
  output: {
    /** Implement nodes: ids of the commits that carry the work out. */
    commits?: string[];
    /** Review nodes, and gates where the user found something: what was found. */
    findings?: Finding[];
    /** Gate nodes: what the user reported back, in a few words. */
    note?: string;
    /** Gate nodes: pictures with the note. */
    attachments?: Attachment[];
    /** Implement and tests tasks: the commands run to check the work, and what came of them. */
    evidence?: Evidence[];
  };
  /** Paths the work still expects to change (files or folders, possibly not existing yet). */
  plans: string[];
  /** Gates: where the message with the user's result is. */
  delivery?: Sent;
  /** When its output was last corrected after delivery. */
  amended?: string;
  /** Dropped as no longer needed: when, by whom ("user", or an agent's name), why. */
  dropped?: { at: string; by: string; reason: string };
}

/** What a question is about, shown as a chip with it. */
export interface AskContext {
  kind: 'task' | 'finding' | 'file' | 'lines' | 'attachment';
  label: string;
  /** attachment: its id in the service's store. */
  id?: string;
  /** file and lines: the repository path. */
  path?: string;
  /** lines: the first and last line. */
  from?: number;
  to?: number;
}

/**
 * One question about a piece of work and its answer. A temporary agent answers from the
 * records and the code; it asks the agent that did the work only when that agent is idle.
 */
/** A conversation about a task; its questions share one temporary agent while it lives. */
export interface Thread {
  id: string;
  /** The user ended it; a new question starts a new thread. */
  ended: boolean;
  /** Its temporary agent runs, so a follow-up keeps its context. */
  live: boolean;
  conclusion?: string;
  turns: Turn[];
}

export interface Turn {
  /** Its id with the service ("q3"); absent in the sample. */
  id?: string;
  /** The agent that asked; absent when the user did. */
  by?: string;
  question: string;
  context: AskContext[];
  /** Null while it comes. */
  answer: string | null;
  /** While it comes: how far it has got. */
  step?: AskStep;
  /** step 'blocked': the agent that waits for a person in its terminal; the question waits too. */
  blocked?: { agent: string; session: string };
  /** The agent that did the work: asked, or not asked because it was busy or gone. */
  consulted?: { agent: string; outcome: 'asked' | 'busy' | 'ended' };
  at: string;
  /** No answer will come, and why. */
  failed?: { code: ReasonCode; reason: string };
  /** A request for the thread's conclusion rather than a question. */
  conclude?: boolean;
}

/** Why a message was not delivered, or a question got no answer. */
export type ReasonCode = 'no-manager' | 'manager-gone' | 'runtime-down' | 'runtime-error' | 'no-workspace' | 'timeout' | 'restarted' | 'blocked' | 'not-ready';

export type AskStep = 'starting' | 'reading' | 'asking' | 'blocked';

export interface Commit {
  /** Short hash, or 'uncommitted' for work not committed yet. */
  id: string;
  subject: string;
  date: string;
  author: string;
  live: boolean;
  /** It belongs to its task because someone linked it, not by its own `Task:` trailer. */
  linked?: boolean;
  /** The same change as this commit of a branch the unit left (a rebase): its old hash. */
  rebasedFrom?: string;
  files: FileChange[];
}

export interface FileChange {
  path: string;
  status: 'A' | 'M' | 'D';
  /**
   * The whole file before this commit; null when the commit adds it. The service's snapshot
   * leaves the texts out (null here, `lines` empty): a unit's come with its code (`loadCode`).
   */
  before: string | null;
  /** The whole file after this commit; null when the commit deletes it. */
  after: string | null;
  /** Lines added and removed: from the service, or worked out from the texts. */
  add: number;
  del: number;
  /** Unified-diff lines including `@@ -a,b +c,d @@ context` hunk headers; from the texts. */
  lines: string[];
}
