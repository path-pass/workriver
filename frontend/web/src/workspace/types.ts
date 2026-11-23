import type { Repo } from '@/code/types'
import type { Chore } from '@/work/types'
import type { Project, BacklogFinding, Intent, Template } from '@/planning/types'
import type { Space, Session } from '@/agents/types'
import type { MachineAlert, Machine } from '@/machines/types'
import type { ManagerMessage } from '@/conversations/types'
import type { RecentRun } from '@/commands/types'

// The shape of what the UI shows: what the workriver service answers on /api/workspace,
// and what src/mock/demo.json holds for the sample.

export interface Workspace {
  /** Every repository a unit works in. */
  repos: Repo[];
  builtAt: string;
  /** Commits, trees and other Git-derived fields follow on the next change. */
  gitPending?: boolean;
  /** The agent runtime's name, as its adapter in the service gives it: the texts that must name
   *  the runtime show it (i18n's setRuntime). */
  runtime: string;
  intents: Intent[];
  /** One per agent. What it is working on comes from the tasks; only this does not. */
  sessions: Session[];
  /** Registered folders of repositories. What names none of them is of "No project". */
  projects?: Project[];
  /** Deferred findings that came from outside workriver's tasks (an earlier review, a notes file). */
  backlog?: BacklogFinding[];
  /** Chores that belong to no intent: a project's (`project`), or nobody's. */
  chores?: Chore[];
  /** Every workspace, open and closed, in the order they were made. */
  workspaces?: Space[];
  /** The other machines followed. */
  machines?: Machine[];
  /** Machines past an alert threshold now (the user's view only). */
  alerts?: MachineAlert[];
  /** The user's messages to managers, with their replies, oldest first. */
  messages?: ManagerMessage[];
  /** The commands the user ran from the page, newest first (the user's view only). */
  runs?: RecentRun[];
  /** The templates a unit can be made with: the user's, then each built-in the user has none of. */
  templates?: Template[];
}
