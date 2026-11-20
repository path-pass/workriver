import type { Repo } from '@/code/types'
import type { Chore } from '@/work/types'
import type { Project, BacklogFinding, Intent } from '@/planning/types'
import type { RuntimeWorkspace, Session } from '@/agents/types'
import type { MachineAlert, Machine } from '@/machines/types'
import type { ManagerMessage } from '@/conversations/types'
import type { RecentRun } from '@/commands/types'

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
  /** The user's messages to managers, with their replies, oldest first. */
  messages?: ManagerMessage[];
  /** The commands the user ran from the page, newest first (the user's view only). */
  runs?: RecentRun[];
}
