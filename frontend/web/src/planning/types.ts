// Projects, their intents and the units that serve them.
import type { Thread } from '@/conversations/types'
import type { RuntimeState } from '@/agents/types'
import type { Commit } from '@/code/types'
import type { Chore, Finding, Task } from '@/work/types'

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
 * A goal in a project and the units that serve it. It runs in a runtime workspace
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

export type UnitStatus = 'active' | 'planned' | 'merged';

/** A piece of an intent that can be implemented on its own, on its own branch. */
export interface Unit {
  id: string;
  /** The intent's own work (research, design), recorded before research units: shown on the
   *  intent's page, with no repository or branch, and never in the rail. */
  intentLevel?: boolean;
  /** Research and design, with no repository, branch or code; closed by hand, never merged.
   *  Spike: an experiment with code on a spike/ branch, shown like code, never merged. Absent: code. */
  kind?: 'research' | 'spike';
  title: string;
  /** One sentence on what the unit is for; may be empty. */
  purpose: string;
  /** Path of its repository, one of `Workspace.repos`. */
  repo: string;
  /** The machine its repository is on; absent: this one. */
  machine?: string;
  /** The branch the work happens on; null once merged. */
  branch: string | null;
  /** The branch it leaves from and is merged into (main). */
  base?: string;
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
  /** Ready, and every change of it is already in its base (or the base's upstream): it looks
   *  merged by squash, which git cannot show. Where, and the commit there that brought it. */
  squash?: { into: string; commit: string; tip: string };
  /** Ready to merge for longer than the service's ready period, not merged: since when. */
  overdue?: string;
  /** When it was recorded as merged. */
  merged?: string;
  /** Conversations about the unit as a whole (asked with no task picked), oldest first. */
  threads?: Thread[];
  /** Open: the files it changes that another open unit of its project, in the same
   *  repository, changes too, with those units (the service compares their changes). */
  overlaps?: { path: string; units: string[] }[];
}
