// Workspaces as the page reads them: workriver's own records (`ws1`), each in one project,
// with the seat of its manager. What the rail shows of them, who can be seated, and which
// of them wait for a manager.
import { createContext, useContext } from 'react'
import type { Seat, Session, Space } from '@/agents/types'
import type { Intent, Project, Unit } from './types'
import type { Workspace } from '@/workspace/types'
import { RUN_GROUP } from '@/api/actions'

/** Every workspace, for what names one by its id far from the data (a copy's line, an inbox row). */
export const Spaces = createContext<Space[]>([])

/** A workspace by its id, wherever the page is. */
export function useSpace(id: string | undefined): Space | undefined {
  const all = useContext(Spaces)
  return id ? all.find(w => w.id === id) : undefined
}

const stateOf = (i: Intent) => i.state ?? 'active'

/** Whether a seat's session runs now. */
export const runs = (m: Seat | undefined): m is Seat => !!m && m.state !== 'gone'

/** A workspace's active intents, in the order the data has them. */
export const activeIn = (intents: Intent[], space: string) => intents.filter(i => stateOf(i) === 'active' && i.workspace === space)

/** Open workspaces with an active intent and no manager running: they wait for the user to seat one. */
export function unmanaged(ws: Workspace): Space[] {
  return (ws.workspaces ?? []).filter(w => !w.closed && !runs(w.manager) && activeIn(ws.intents, w.id).length > 0)
}

/** One workspace of the rail: its active intents, each with the units shown. */
export interface RailSpace {
  space: Space
  intents: { intent: Intent; units: Unit[] }[]
}

/** One project of the rail: its open workspaces with their active intents, then its roadmap
 *  and what is done. `project` null: "No project" (planned intents of none, and what else
 *  names none). `loose`: active intents in no open workspace of it (records that say none). */
export interface RailProject {
  project: Project | null
  spaces: RailSpace[]
  loose: { intent: Intent; units: Unit[] }[]
  planned: Intent[]
  done: { intent: Intent; units: Unit[] }[]
}

/**
 * The rail's four levels: project → workspace → intent → unit. `groups` gives each intent
 * the units to show (a search or a filter may leave it out: then it is not in `groups`);
 * `planned` is the roadmap rows to show, in order. A workspace shows even with nothing
 * active in it (a search leaves it out when nothing in it matches: `searching`). "No
 * project" comes last and only when it holds something.
 */
export function railOf(projects: Project[], spaces: Space[], groups: { intent: Intent; units: Unit[] }[], planned: Intent[], only: string | null, searching = false): RailProject[] {
  const known = new Set(projects.map(p => p.id))
  const of = (i: Intent) => (i.project && known.has(i.project) ? i.project : '')
  const section = (project: Project | null): RailProject => {
    const id = project?.id ?? ''
    const mine = groups.filter(g => of(g.intent) === id)
    const open = spaces.filter(w => !w.closed && w.project === id && !!project)
    const active = mine.filter(g => stateOf(g.intent) === 'active')
    const inSpace = (w: Space) => active.filter(g => g.intent.workspace === w.id)
    return {
      project,
      spaces: open.map(w => ({ space: w, intents: inSpace(w) })).filter(s => !searching || s.intents.length > 0),
      loose: active.filter(g => !open.some(w => w.id === g.intent.workspace)),
      planned: planned.filter(i => of(i) === id),
      done: mine.filter(g => stateOf(g.intent) === 'done'),
    }
  }
  const shown = projects.filter(p => !only || p.id === only).map(section)
  const none = section(null)
  const empty = !none.loose.length && !none.planned.length && !none.done.length
  return only || empty ? shown : [...shown, none]
}

/** An agent that can be seated: a session running now, with what the records know of it. */
export interface Candidate {
  s: Session
  /** What the seating names: its session (the sample has only names). */
  key: string
  /** The workspace whose seat it holds now, if any. */
  seat?: Space
  /** The tasks it has: running or to do. */
  tasks: { unit: string; id: string }[]
}

/** The agents running now that the user can seat: not a question's temporary agent, not a
 *  command shell's reviewer; those with no tasks and no seat first (a manager-to-be), by name. */
export function candidates(ws: Workspace): Candidate[] {
  const tasks = ws.intents.flatMap(i => i.units).flatMap(u => u.tasks.map(n => ({ u, n })))
  return ws.sessions
    .filter(s => !s.ended && !/^why-q\d+$/.test(s.agent) && s.group?.label !== RUN_GROUP)
    .map((s): Candidate => ({
      s,
      key: s.session ?? s.agent,
      seat: (ws.workspaces ?? []).find(w => w.manager && w.manager.session === (s.session ?? s.agent)),
      tasks: tasks
        .filter(({ n }) => (n.state === 'running' || n.state === 'todo') && (s.session && n.agentSession ? n.agentSession === s.session : n.agent === s.agent))
        .map(({ u, n }) => ({ unit: u.id, id: n.taskId ?? n.id })),
    }))
    .sort((a, b) => Number(a.tasks.length > 0) - Number(b.tasks.length > 0) || a.s.agent.localeCompare(b.s.agent))
}

/** What a change of a workspace's seat sends as the seat it replaces: the one the user was shown. */
export const replaces = (space: Space) => (space.manager ? { session: space.manager.session, since: space.manager.since } : null)

/** The proposal is the seated session itself, shown in another folder (or on another
 *  machine): a new terminal reports it, and confirming it is a new appointment. */
export function movedSeat(space: Space, c: Candidate | undefined): boolean {
  const m = space.manager
  return !!m && !!c && c.key === m.session && ((c.s.folder ?? '') !== m.folder || (c.s.machine ?? '') !== (m.machine ?? ''))
}

/** Whether `folder` is the folder `root` or inside it. */
function within(folder: string, root: string): boolean {
  const r = root.replace(/\/+$/, '')
  return !!r && (folder === r || folder.startsWith(`${r}/`))
}

/** The manager select's list: `here`, the agents started in the workspace's project folder or
 *  inside it, on this machine (a project's folder is on the machine workriver runs on), and
 *  those in `keep` (the agent seated now, the one preset) wherever they run; `rest`, the
 *  others. Each keeps the order of `list`. */
export function seatList(list: Candidate[], project: string, keep: string[]): { here: Candidate[]; rest: Candidate[] } {
  const near = (c: Candidate) => keep.includes(c.key) || (!c.s.machine && !!c.s.folder && within(c.s.folder, project))
  return { here: list.filter(near), rest: list.filter(c => !near(c)) }
}

/** Home folders as the systems put them: `/Users/<name>`, `/home/<name>`, `/root`. */
const HOME = /^(\/Users\/[^/]+|\/home\/[^/]+|\/root)(?=\/|$)/

/** A folder said short: inside a registered project (on this machine) its name and the path
 *  under it (the deepest project holding it; just the name at its root), else under a home
 *  folder `~/…`, else whole. */
export function shortFolder(folder: string, machine: string | undefined, projects: Project[]): string {
  const p = machine ? undefined : projects.filter(p => within(folder, p.id)).sort((a, b) => b.id.length - a.id.length)[0]
  if (p) {
    const under = folder.slice(p.id.replace(/\/+$/, '').length + 1)
    return under ? `${p.title}/${under}` : p.title
  }
  const home = HOME.exec(folder)
  return home ? `~${folder.slice(home[0].length)}` : folder
}
