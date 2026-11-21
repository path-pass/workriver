// The side panel, right of the main area: what asks something of the user (the inbox),
// where the agents are, what was committed lately, the backlog, the machines' CPU and memory,
// or the conversation with a workspace's manager – one at a time. Neither is looked at all the time, so the panel folds
// to a thin strip; on a narrow screen the same content opens as a sheet over the page.
import { useEffect, useRef, type ReactNode } from 'react'
import { Button } from '@/ui/primitives/button'
import { ScrollArea } from '@/ui/primitives/scroll-area'
import { Sheet, SheetContent, SheetTitle } from '@/ui/primitives/sheet'
import type { Pick } from '@/work/types'
import { useT } from '@/i18n'
import { useResize } from '@/lib/useResize'
import { SIDE_WIDTH } from '@/config/defaults'
import type { Workspace } from '@/workspace/types'
import { BacklogList } from '@/attention/Backlog'
import { InboxList } from '@/attention/Inbox'
import { MachinesList } from '@/machines/Machines'
import { RecentRuns, RunBox } from '@/commands/Run'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { Agents } from '@/agents/Agents'
import type { CommitFocus } from '@/code/types'
import { Commits } from '@/code/Commits'

export type Section = 'inbox' | 'agents' | 'commits' | 'backlog' | 'machines' | 'run' | 'talk'
export const SECTIONS: Section[] = ['inbox', 'agents', 'commits', 'backlog', 'machines', 'run', 'talk']

interface ContentProps {
  ws: Workspace
  /** The unit and pick on screen: the agent line that leads there shows as picked. */
  here: { unit: string; pick: Pick } | null
  onOpen: (unit: string, pick: Pick) => void
  section: Section
  onCommit: (unit: string, task: string, commit: string) => void
  commit: CommitFocus | null
  /** The project picked in the rail, or null for all: the backlog shows its findings. */
  project: string | null
  /** The Manager section, made by the caller: it lays itself out (its box stays at the bottom). */
  talk: ReactNode
  /** Shows another section (a machine's alert in Needs you opens Machines). */
  onSection: (s: Section) => void
  /** Opens Manager on a workspace's conversation (unread messages in Needs you). */
  onTalk: (workspace: string) => void
}

function Content({ ws, here, onOpen, section, onCommit, commit, project, onSection, onTalk }: Omit<ContentProps, 'talk'>) {
  const { t, num } = useT()
  const live = ws.sessions.filter(s => !s.ended).length
  return (
    <div className="sidepane flex flex-col px-2 pb-6">
      {section === 'inbox' ? (
        <section aria-labelledby="side-inbox">
          <h2 id="side-inbox" className="mb-2 px-2.5 text-sm font-[650]">{t('inbox.title')}</h2>
          <InboxList ws={ws} onOpen={onOpen} onMachines={() => onSection('machines')} onTalk={onTalk} />
        </section>
      ) : section === 'machines' ? (
        <section aria-labelledby="side-machines">
          <h2 id="side-machines" className="mb-2 px-2.5 text-sm font-[650]">{t('metrics.title')}</h2>
          <MachinesList />
        </section>
      ) : section === 'backlog' ? (
        <section aria-labelledby="side-backlog">
          <h2 id="side-backlog" className="mb-2 px-2.5 text-sm font-[650]">{t('backlog.title')}</h2>
          <BacklogList ws={ws} project={project} onOpen={onOpen} />
        </section>
      ) : section === 'run' ? (
        <section aria-labelledby="side-run">
          <h2 id="side-run" className="mb-2 px-2.5 text-sm font-[650]">{t('run.section')}</h2>
          <RunBox ws={ws} unit={here?.unit ?? null} project={project} />
          <RecentRuns runs={ws.runs ?? []} />
        </section>
      ) : section === 'commits' ? (
        <section aria-labelledby="side-commits">
          <h2 id="side-commits" className="mb-2 px-2.5 text-sm font-[650]">{t('commits.title')}</h2>
          <Commits ws={ws} here={here} onCommit={onCommit} commit={commit} project={project} />
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

/** Manager keeps its box at the bottom, so it scrolls inside itself; the others scroll whole. */
function Body(props: ContentProps) {
  if (props.section === 'talk') return props.talk
  return <ScrollArea className="min-h-0 flex-1 [&>[data-slot=scroll-area-viewport]>div]:block!"><Content {...props} /></ScrollArea>
}

/** The buttons work as a switch: one section at a time; the open one again folds it. */
export function SideButtons({ active, onPick }: { active: Section | null; onPick: (s: Section) => void }) {
  const { t } = useT()
  const button = (s: Section, icon: string, label: string) => (
    <Tip text={label}>
      <Button variant="ghost" size="icon-sm" className="aria-pressed:bg-you-bg aria-pressed:text-you-ink" aria-label={label} aria-pressed={active === s} onClick={() => onPick(s)}>
        <Icon name={icon} />
      </Button>
    </Tip>
  )
  return <>{button('inbox', 'inbox', t('inbox.title'))}{button('agents', 'agents', t('agents.title'))}{button('commits', 'commits', t('commits.title'))}{button('backlog', 'backlog', t('backlog.title'))}{button('machines', 'machine', t('metrics.title'))}{button('run', 'terminal', t('run.section'))}{button('talk', 'talk', t('talk.title'))}</>
}

interface PanelProps extends Omit<ContentProps, 'section'> {
  /** The section shown; null when folded. */
  section: Section | null
  onClose: () => void
  /** Its width once a drag settles, so a graph beside it can refit. */
  onWidth?: (w: number) => void
}

/** Wide screens: a column of its own that folds to a strip of two buttons. */
export function SidePanel({ section, onSection, onClose, onWidth, ...content }: PanelProps) {
  const { t } = useT()
  // The left edge drags the width; double-click: the default.
  const size = useResize({ key: 'workriver.side.width', variable: '--side-w', ...SIDE_WIDTH, edge: 'left' })
  useEffect(() => onWidth?.(size.size), [onWidth, size.size])
  if (!section)
    return (
      <aside className="sticky top-0 flex h-screen w-11 flex-col items-center gap-1 border-l border-line bg-surface pt-3 max-[860px]:hidden">
        <SideButtons active={null} onPick={onSection} />
      </aside>
    )
  return (
    <aside className="sticky top-0 flex h-screen w-[var(--side-w,340px)] flex-col border-l border-line bg-surface max-[860px]:hidden">
      <div {...size.handle} aria-label={t('side.resize')} className="edge" data-edge="left" />
      <div className="flex items-center gap-1 px-2 pt-3 pb-2">
        <SideButtons active={section} onPick={onSection} />
        <Tip text={t('side.close')}>
          <Button variant="ghost" size="icon-sm" className="ml-auto" aria-label={t('side.close')} onClick={onClose}><Icon name="fold" /></Button>
        </Tip>
      </div>
      <Body section={section} onSection={onSection} {...content} />
    </aside>
  )
}

/** Narrow screens: the same content as a sheet over the page, with the same switch. */
export function SideSheet({ section, onSection, onClose, ...content }: PanelProps) {
  const { t } = useT()
  // The sheet has no trigger of its own (several buttons open it): focus goes back to the one used.
  const opener = useRef<HTMLElement | null>(null)
  return (
    <Sheet open={!!section} onOpenChange={o => { if (!o) onClose() }}>
      <SheetContent
        aria-describedby={undefined}
        onOpenAutoFocus={() => { opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null }}
        onCloseAutoFocus={e => { if (opener.current?.isConnected) { e.preventDefault(); opener.current.focus() } }}
      >
        <div className="flex items-center gap-1 px-2 pt-3 pb-2">
          <SheetTitle className="sr-only">{t('side.title')}</SheetTitle>
          <SideButtons active={section} onPick={onSection} />
          <Button variant="ghost" size="icon-sm" className="ml-auto" aria-label={t('side.close')} onClick={onClose}><Icon name="no" /></Button>
        </div>
        {section && <Body section={section} onSection={onSection} {...content} />}
      </SheetContent>
    </Sheet>
  )
}
