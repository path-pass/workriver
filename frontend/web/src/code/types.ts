// A unit's code: its repository, its commits and their files, and how the code layer
// shows them.

export interface Repo {
  /** Where it is on disk; a unit names its repository by this. */
  path: string;
  /** The machine it is on; absent: this one. */
  machine?: string;
  name: string;
  /** The code as a tree of folders and files. The root has an empty path. */
  modules: ModuleNode;
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
  /** A merge commit (the base merged into the branch): its files are not work of its own. */
  merge?: boolean;
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

/** A unit's file texts, by commit id and path, as of its `code`. */
export interface UnitCode { code: string; files: Map<string, FileChange> }

// ---- the file tree: where the unit changed, plans to change, or is writing right now ----

/** One row of the file tree: a file, or a folder; a chain of single folders is one row. */
export interface TreeRow {
  /** The deepest path the row stands for; ROOT for the root. */
  id: string
  /** A file's name, or folder names joined by '/' for a chain. */
  name: string
  folder: boolean
  children: string[]
  add: number
  del: number
  /** Files the unit changed: how. */
  status?: 'A' | 'M' | 'D'
  /** Work not done yet expects to change it, and nothing has yet. */
  planned: boolean
  /** Not in the repository yet: the unit adds it. */
  ghost: boolean
  /** Agents writing it, or something inside it, right now. */
  writers: string[]
  /** The unit changed, plans to change or is writing it, or something inside it. */
  changed: boolean
  /** Part of what is picked (with nothing picked: of the whole unit). */
  inPick: boolean
}

/**
 * Lines picked in the code, by their numbers: in the single-file view, in the file as shown;
 * in the list, in the file as `commit` (an index into the unit's commits) left it.
 */
export interface Lines { path: string; from: number; to: number; commit?: number }

export interface FileTree {
  rows: Map<string, TreeRow>
  /** Folders open by default: those holding the pick's files and (unless `narrow`) files being written. */
  open: string[]
}

export interface FileDelta {
  /** Indexes of the scope's commits that touch the file, oldest first. */
  touched: number[]
  before: string | null
  after: string | null
  status: 'A' | 'M' | 'D'
  add: number
  del: number
}
