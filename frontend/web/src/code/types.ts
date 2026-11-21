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
  /**
   * A merge that brought the base into the branch: its files, against its first parent as
   * every commit's, are the base's work: no task's change holds it, and its lines are the
   * base's. A merge of any other branch is not one: what it brought is the branch's.
   */
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
  /**
   * The whole file after this commit; null when the commit deletes it. Null on both sides of
   * a binary change and on a side that is a submodule's pointer (no text): see `binary` and
   * the modes.
   */
  after: string | null;
  /** Lines added and removed: from the service, or worked out from the texts. */
  add: number;
  del: number;
  /** Unified-diff lines including `@@ -a,b +c,d @@ context` hunk headers; from the texts. */
  lines: string[];
  /**
   * Each side's git entry: its mode ('100644'; '000000' where the side does not exist) and
   * object id (zeros where it does not exist, and for uncommitted work's side, which git has
   * not hashed). Whether a file changed over several commits is whether these differ.
   */
  beforeMode: string;
  beforeId: string;
  afterMode: string;
  afterId: string;
  /** Either side is binary (git: "Binary files differ"): no texts, no line counts. */
  binary?: boolean;
}

/** A submodule's pointer at the ends of a change: its commits, null where an end is not one. */
export interface Pointer { from: string | null; to: string | null }

/** What a change is that has no lines: a binary file's, or a submodule pointer's. */
export interface Kind { binary?: boolean; pointer?: Pointer }

/**
 * A unit's net change against its base, as `git diff base...head` names it: from where the
 * branch meets its base now (merged: where it met it before) to its last commit. `id` is
 * "from..tip": it changes when the base moves, the commits the same.
 */
export interface Net { id: string; files: FileChange[] }

/** A unit's file texts, by commit id and path, and its net change's, as of its `code`. */
export interface UnitCode { code: string; files: Map<string, FileChange>; net?: Net }

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
 * in the list, and in a part of the single-file view, in the file as `commit` (an index into
 * the unit's commits) left it.
 */
export interface Lines { path: string; from: number; to: number; commit?: number }

export interface FileTree {
  rows: Map<string, TreeRow>
  /** Folders open by default: those holding the pick's files and (unless `narrow`) files being written. */
  open: string[]
}

export interface FileDelta {
  /** Indexes of the commits the change comes from, oldest first: the scope's that touch the file. */
  touched: number[]
  /** The last part's texts, and the commit its `after` is the file as of (`blame`). */
  before: string | null
  after: string | null
  upTo: number
  status: 'A' | 'M' | 'D'
  /** Over all parts. */
  add: number
  del: number
  /**
   * The stretches compared one by one, oldest first: one, unless another commit touched the
   * file between the scope's (a merge of the base, other work), where their texts do not chain.
   */
  parts: FilePart[]
}

/** One stretch of a file's change; how it changed is from its ends' entries (a binary file and a
 *  submodule pointer have no texts to tell it by). */
export interface FilePart extends Kind { touched: number[]; upTo: number; status: 'A' | 'M' | 'D'; before: string | null; after: string | null; add: number; del: number }

/** A commit opened from the panel: the code block shows it. `at` changes with every request. */
export type CommitFocus = { unit: string; id: string; at: number }
