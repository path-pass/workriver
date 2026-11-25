// The icon set, and the marks and counts made of it.
import { Archive, Ellipsis, ArrowLeft, GitCompareArrows, ListFilter, Ban, FoldVertical, UnfoldVertical, EyeOff, Server, ArrowUp, Bot, ExternalLink, GripVertical, Play, Telescope, Crown, DraftingCompass, Scale, FlaskConicalOff, GitCommitHorizontal, Link2, Check, CircleDashed, ChevronRight, Circle, Clock, Copy, Expand, Eye, FileText, GitMerge, FlaskConical, Hand, ImagePlus, Inbox, ListChecks, ListTodo, MessageSquare, Moon, OctagonAlert, PanelLeftClose, PanelLeftOpen, PanelRightClose, Plus, SquareTerminal, Users, Pencil, Rocket, X, ThumbsDown, ThumbsUp, type LucideIcon } from 'lucide-react'
import { useT } from '@/i18n'
import { cn } from '@/lib/utils'
import { Tip } from './Tip'

// Who raised it is an icon; so is the state of a piece of work. All from lucide, one size, one stroke.
const ICONS: Record<string, LucideIcon> = {
  check: Check,
  ring: Circle,
  pen: Pencil,
  later: Clock,
  planned: CircleDashed,
  agent: Bot,
  manager: Crown,
  'gate-deploy': Rocket,
  'gate-test': Hand,
  'gate-decide': Scale,
  design: DraftingCompass,
  research: Telescope,
  backlog: Archive,
  machine: Server,
  blind: EyeOff,
  grip: GripVertical,
  play: Play,
  link: ExternalLink,
  blocked: Hand,
  blocker: OctagonAlert,
  chore: ListTodo,
  inbox: Inbox,
  agents: Users,
  fold: PanelRightClose,
  idle: Moon,
  copy: Copy,
  permalink: Link2,
  terminal: SquareTerminal,
  talk: MessageSquare,
  'thumbs-up': ThumbsUp,
  'thumbs-down': ThumbsDown,
  send: ArrowUp,
  attach: ImagePlus,
  file: FileText,
  plus: Plus,
  untested: FlaskConicalOff,
  dropped: Ban,
  commits: GitCommitHorizontal,
  rail: PanelLeftClose,
  'rail-open': PanelLeftOpen,
  no: X,
  // Every review has the one icon; which kind it is, the node's text says.
  'security-review': Eye,
  'code-review': Eye,
  'architecture-review': Eye,
  test: FlaskConical,
  grew: Expand,
  chev: ChevronRight,
  merge: GitMerge,
  back: ArrowLeft,
  foldLines: FoldVertical,
  unfold: UnfoldVertical,
  // Another open unit changes the same file.
  overlap: GitCompareArrows,
  // The rail's filter by kind, size and tag.
  filter: ListFilter,
  // A workspace's menu in the rail.
  more: Ellipsis,
  // A unit's template and the steps it placed.
  template: ListChecks,
}

export function Icon({ name, className }: { name: string; className?: string }) {
  const Glyph = ICONS[name] ?? Circle
  return <Glyph aria-hidden size={16} strokeWidth={1.5} className={cn('ic', className)} />
}

/** An icon that says something by itself: named for assistive tech, explained on hover. */
export function Mark({ name, label }: { name: string; label: string }) {
  return (
    <Tip text={label}>
      <span role="img" aria-label={label} className="inline-flex flex-none"><Icon name={name} className="size-3.5" /></span>
    </Tip>
  )
}

/** How many findings of a review wait for the user: the one violet count, wherever it shows. */
export function Waiting({ count }: { count: number }) {
  const { t, num } = useT()
  return (
    <span className="count">
      <span aria-hidden>{num(count)}</span>
      <span className="sr-only">{t('finding.waiting', { count })}</span>
    </span>
  )
}
