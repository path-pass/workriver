// The side panel, right of the main area: what asks something of the user (the inbox),
// where the agents are, or what was committed lately – one at a time. Neither is looked at all the time, so the panel folds
// to a thin strip; on a narrow screen the same content opens as a sheet over the page.
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Sheet, SheetContent, SheetTitle } from '@/components/ui/sheet'
import { agents, baseName, commitStats, feed, ms, writingNow, type Pick } from '@/data'
import { useT } from '@/i18n'
import type { Workspace } from '@/types'
import { InboxList } from './Inbox'
import { Icon, Mark, TerminalButton, Tip, Who } from './bits'

export type Section = 'inbox' | 'agents' | 'commits'

/** A commit opened from the panel: the code block shows it. `at` changes with every request. */
export type CommitFocus = { unit: string; id: string; at: number }

interface ContentProps {
  ws: Workspace
  /** The unit and pick on screen: the agent line that leads there shows as picked. */
  here: { unit: string; pick: Pick } | null
  onOpen: (unit: string, pick: Pick) => void
  section: Section
  onCommit: (unit: string, task: string, commit: string) => void
  commit: CommitFocus | null
}

/** Every live agent on one line: where it works, holds work, or that it is idle. */
function Agents({ ws, here, onOpen }: Omit<ContentProps, 'section' | 'onCommit' | 'commit'>) {
  const { t } = useT()
  return (
    <div className="flex flex-col">
      {agents(ws).map(({ agent, state, at, manager }) => (
        // The line opens the agent's work; the icon at its end, its terminal.
        <div key={agent} className="agent-row">
        <button
          type="button"
          className="agent"
          data-place={state}
          disabled={!at}
          aria-pressed={!!at && here?.unit === at.unit.id && here.pick?.id === at.node.id}
          onClick={() => at && onOpen(at.unit.id, { kind: 'task', id: at.node.id })}
        >
          <Who name={agent} live={state === 'working' && !!at?.node.writing} />
          <span className="flex-none">{agent}</span>
          {manager && <Mark name="manager" label={t('manager.mark')} />}
          <span className="at">
            {state === 'idle'
              ? <Mark name="idle" label={t('agents.idle')} />
              : <><span className="truncate">{at?.unit.title}</span>{at?.node.taskId && <span className="flex-none font-mono">#{at.node.taskId}</span>}</>}
          </span>
        </button>
        <TerminalButton agent={agent} />
        </div>
      ))}
    </div>
  )
}

/** Commits across every unit, newest first, by day; on top, who is writing right now. */
function Commits({ ws, here, onCommit, commit: focus }: Omit<ContentProps, 'section' | 'onOpen'>) {
  const { t, num, waited, date } = useT()
  const rows = feed(ws)
  const now = writingNow(ws)
  // Days by the calendar: today, yesterday, then dates.
  const dayOf = (s: string) => new Date(ms(s)).toDateString()
  const label = (s: string) => {
    const d = new Date(ms(s)), today = new Date()
    const yesterday = new Date(today); yesterday.setDate(today.getDate() - 1)
    return d.toDateString() === today.toDateString() ? t('commits.today') : d.toDateString() === yesterday.toDateString() ? t('commits.yesterday') : date(s, false)
  }
  const days = [...new Set(rows.map(r => dayOf(r.commit.date)))]
  return (
    <div className="flex flex-col gap-4">
      {now.length > 0 && (
        <section aria-labelledby="commits-now">
          <h3 id="commits-now" className="mb-1 px-2.5 text-xs font-semibold text-amber-ink">{t('commits.writing')}</h3>
          {now.map(w => (
            <div key={w.unit.id + w.node.id} className="crow" data-live>
              <Who name={w.node.agent ?? '?'} live />
              <span className="min-w-0">
                <span className="block truncate font-mono text-xs">{w.files.map(baseName).join(' ')}</span>
                <span className="block truncate text-xs text-ink-2">{w.unit.title}{w.node.taskId && ` #${w.node.taskId}`}</span>
              </span>
            </div>
          ))}
        </section>
      )}
      {days.map(day => {
        const list = rows.filter(r => dayOf(r.commit.date) === day)
        return (
          <section key={day} aria-label={label(list[0].commit.date)}>
            <h3 className="mb-1 px-2.5 text-xs font-semibold text-ink-2">{label(list[0].commit.date)}</h3>
            {list.map(({ unit, commit, task }) => {
              const s = commitStats(commit)
              return (
                <button
                  key={unit.id + commit.id}
                  type="button"
                  className="crow"
                  disabled={!task}
                  aria-pressed={focus?.id === commit.id && here?.unit === unit.id}
                  onClick={() => task && onCommit(unit.id, task.id, commit.id)}
                >
                  <Who name={commit.author || '?'} />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate">{commit.subject}</span>
                    <span className="flex items-center gap-1.5 text-xs text-ink-2">
                      <span className="truncate">{unit.title}</span>
                      {task?.state === 'unregistered'
                        ? <span className="flex flex-none items-center gap-0.5 text-del"><Icon name="no" className="size-3" />{t('need.loose')}</span>
                        : task?.taskId && <span className="flex-none font-mono">#{task.taskId}</span>}
                      <span className="flex-none font-mono"><span className="text-add">+{num(s.add)}</span> <span className="text-del">−{num(s.del)}</span></span>
                    </span>
                  </span>
                  <span className="flex-none text-xs text-ink-2">{waited(ms(commit.date))}</span>
                </button>
              )
            })}
          </section>
        )
      })}
    </div>
  )
}

function Content({ ws, here, onOpen, section, onCommit, commit }: ContentProps) {
  const { t, num } = useT()
  const live = ws.sessions.filter(s => !s.ended).length
  return (
    <div className="sidepane flex flex-col px-2 pb-6">
      {section === 'inbox' ? (
        <section aria-labelledby="side-inbox">
          <h2 id="side-inbox" className="mb-2 px-2.5 text-sm font-[650]">{t('inbox.title')}</h2>
          <InboxList ws={ws} onOpen={onOpen} />
        </section>
      ) : section === 'commits' ? (
        <section aria-labelledby="side-commits">
          <h2 id="side-commits" className="mb-2 px-2.5 text-sm font-[650]">{t('commits.title')}</h2>
          <Commits ws={ws} here={here} onCommit={onCommit} commit={commit} />
        </section>
      ) : (
        <section aria-labelledby="side-agents">
          <h2 id="side-agents" className="mb-2 flex items-baseline gap-1.5 px-2.5 text-sm font-[650]">
            {t('agents.title')}<span className="text-xs font-normal text-ink-2">{num(live)}</span>
          </h2>
          <Agents ws={ws} here={here} onOpen={onOpen} />
        </section>
      )}
    </div>
  )
}

/** The three buttons work as a switch: one section at a time; the open one again folds it. */
export function SideButtons({ active, onPick }: { active: Section | null; onPick: (s: Section) => void }) {
  const { t } = useT()
  const button = (s: Section, icon: string, label: string) => (
    <Tip text={label}>
      <Button variant="ghost" size="icon-sm" className="aria-pressed:bg-you-bg aria-pressed:text-you-ink" aria-label={label} aria-pressed={active === s} onClick={() => onPick(s)}>
        <Icon name={icon} />
      </Button>
    </Tip>
  )
  return <>{button('inbox', 'inbox', t('inbox.title'))}{button('agents', 'agents', t('agents.title'))}{button('commits', 'commits', t('commits.title'))}</>
}

interface PanelProps extends Omit<ContentProps, 'section'> {
  /** The section shown; null when folded. */
  section: Section | null
  onSection: (s: Section) => void
  onClose: () => void
}

/** Wide screens: a column of its own that folds to a strip of two buttons. */
export function SidePanel({ section, onSection, onClose, ...content }: PanelProps) {
  const { t } = useT()
  if (!section)
    return (
      <aside className="sticky top-0 flex h-screen w-11 flex-col items-center gap-1 border-l border-line bg-surface pt-3 max-[860px]:hidden">
        <SideButtons active={null} onPick={onSection} />
      </aside>
    )
  return (
    <aside className="sticky top-0 flex h-screen w-[340px] flex-col border-l border-line bg-surface max-[860px]:hidden">
      <div className="flex items-center gap-1 px-2 pt-3 pb-2">
        <SideButtons active={section} onPick={onSection} />
        <Tip text={t('side.close')}>
          <Button variant="ghost" size="icon-sm" className="ml-auto" aria-label={t('side.close')} onClick={onClose}><Icon name="fold" /></Button>
        </Tip>
      </div>
      <ScrollArea className="min-h-0 flex-1 [&>[data-slot=scroll-area-viewport]>div]:block!"><Content section={section} {...content} /></ScrollArea>
    </aside>
  )
}

/** Narrow screens: the same content as a sheet over the page, with the same switch. */
export function SideSheet({ section, onSection, onClose, ...content }: PanelProps) {
  const { t } = useT()
  return (
    <Sheet open={!!section} onOpenChange={o => { if (!o) onClose() }}>
      <SheetContent aria-describedby={undefined}>
        <div className="flex items-center gap-1 px-2 pt-3 pb-2">
          <SheetTitle className="sr-only">{t('side.title')}</SheetTitle>
          <SideButtons active={section} onPick={onSection} />
          <Button variant="ghost" size="icon-sm" className="ml-auto" aria-label={t('side.close')} onClick={onClose}><Icon name="no" /></Button>
        </div>
        <ScrollArea className="min-h-0 flex-1 [&>[data-slot=scroll-area-viewport]>div]:block!">{section && <Content section={section} {...content} />}</ScrollArea>
      </SheetContent>
    </Sheet>
  )
}
