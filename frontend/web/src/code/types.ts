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
