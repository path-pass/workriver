// Projects, their intents and the units that serve them.
import type { Thread } from '@/conversations/types'
import type { RuntimeState } from '@/agents/types'
import type { Commit, Net } from '@/code/types'
import type { Chore, Finding, Task } from '@/work/types'
import type { Kind, Size } from './classify'

/** A registered folder holding one or more repositories. */
export interface Project {
  /** Its folder, which is what intents and backlog findings name. */
  id: string;
  /** Its short id (`p1`). */
  key?: string;
  title: string;
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
 * active   started in a workspace of its project, whose manager runs it
 * done / dropped
 */
export type IntentState = 'planned' | 'active' | 'done' | 'dropped';

/**
 * A goal in a project and the units that serve it. It runs in a workspace of its project
 * once it is started; a workspace may run several.
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
  /** Its project's folder (a `Project.id`); empty: of no project. */
  project?: string;
  /** More about it; may hold links. */
  note?: string;
  external?: External;
  /** The workspace it runs (or last ran) in, by its id (a `Space.id`); a planned one is in none. */
  workspace?: string;
  /** Planned: its place in its project's roadmap, from 0. */
  order?: number;
  /** Dropped: why. */
  reason?: string;
  /** What kind of work it is, how big it is expected to be, free tags; absent when not set. */
  kind?: Kind;
  size?: Size;
  tags?: string[];
  units: Unit[];
  /** Things agents left for the user that block nobody. Never on a graph. */
  chores: Chore[];
  /** The agent that splits the intent's work and hands it out; absent when its workspace's seat is empty. */
  manager?: string;
  /** The manager's state in the runtime, or 'gone' when it is not running. */
  managerState?: RuntimeState | 'gone';
  /** Its last version (1 when never changed; absent in the sample: 1): an edit's base. */
  version?: number;
  /** Who made that version (an agent's name; null: the user) and when. */
  edited?: { at: string; by: string | null };
}

/** One version of an intent's title, goal and note, and what its change set. */
export interface IntentVersion {
  n: number;
  at: string;
  /** "user", or the agent's session. */
  by: string;
  /** The name the agent goes by. */
  name?: string | null;
  how: 'created' | 'edited' | 'merged';
  title?: string;
  goal?: string;
  /** The whole note, when the version set it. */
  note?: string;
  /** The paragraph it added to the note. */
  added?: string;
  /** The intents merged into it. */
  merged?: string[];
  /** The kind and size it set (null: cleared), the tags it left; absent when it did not change them. */
  kind?: Kind | null;
  size?: Size | null;
  tags?: string[];
}

/** An intent's text as one version left it, with its kind, size and tags. */
export interface IntentText {
  title: string;
  goal: string;
  note: string;
  kind?: Kind | null;
  size?: Size | null;
  tags?: string[];
}

export type UnitStatus = 'active' | 'planned' | 'merged';

/** A piece of an intent that can be implemented on its own, on its own branch. */
export interface Unit {
  id: string;
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
  /** The branch's net change against its base (the code tree with nothing picked); absent where the service could not tell. */
  net?: Net;
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
