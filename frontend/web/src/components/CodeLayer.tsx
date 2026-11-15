// Layer 2, code: the files on the left, the changes on the right, as in a pull request's
// "Files changed". The tree shows where the unit (or the picked piece of work) changed,
// plans to change, or is writing right now; everything else can be hidden. With no file
// picked the right side lists the commits; with one picked it shows that file whole.
import { hotkeysCoreFeature, selectionFeature, syncDataLoaderFeature } from '@headless-tree/core'
import { useTree } from '@headless-tree/react'
import { html } from 'diff2html'
import { useEffect, useMemo, useRef, useState, type Dispatch, type ReactNode, type SetStateAction } from 'react'
import { Button } from '@/components/ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Switch } from '@/components/ui/switch'
import { changeOf, fileTree, ROOT, shownDelta, type Lines, type FileTree, type Pick, type Selection, type TreeRow } from '@/data'
import { useT } from '@/i18n'
import type { FileChange, Unit, ModuleNode } from '@/types'
import { Icon, Tip, Who } from './bits'
import type { CommitFocus } from './Side'

type Change = Omit<FileChange, 'before' | 'after'>

/** diff2html reads unified diffs, so give each file its header back. */
function unified(f: Change): string {
  const a = f.status === 'A' ? '/dev/null' : 'a/' + f.path, b = f.status === 'D' ? '/dev/null' : 'b/' + f.path
  return [`diff --git a/${f.path} b/${f.path}`, `--- ${a}`, `+++ ${b}`, ...f.lines].join('\n') + '\n'
}

function FileDiff({ file, whole = false }: { file: Change; whole?: boolean }) {
  const { t, num } = useT()
  // diff2html escapes the diff's text itself; its output is markup, not repository content.
  // Its own file list is left out: the tree is the list.
  const body = useMemo(
    () => (file.lines.length ? html(unified(file), { drawFileList: false, outputFormat: 'line-by-line', matching: 'lines', diffStyle: 'word' }) : ''),
    [file],
  )
  const head = (
    <>
      <span className="tag" data-tone={file.status === 'A' ? 'add' : file.status === 'D' ? 'del' : undefined}>{t(`change.${file.status}`)}</span>
      <span className="min-w-0 font-mono wrap-anywhere">{file.path}</span>
      <span className="ml-auto font-mono whitespace-nowrap">
        <span className="text-add">+{num(file.add)}</span> <span className="text-del">−{num(file.del)}</span>
      </span>
    </>
  )
  if (!body) return <div className="fview"><div className="fhead">{head}</div></div>
  // The whole file is one view; it does not fold away.
  if (whole) return <div className="fview"><div className="fhead">{head}</div><div className="diff" dangerouslySetInnerHTML={{ __html: body }} /></div>
  return (
    <Collapsible className="fview" defaultOpen>
      <CollapsibleTrigger className="fhead"><Icon name="chev" className="chev" />{head}</CollapsibleTrigger>
      <CollapsibleContent><div className="diff" dangerouslySetInnerHTML={{ __html: body }} /></CollapsibleContent>
    </Collapsible>
  )
}

/** What a file row says about the file: how it changed, or that a change is still to come. */
function Status({ r }: { r: TreeRow }) {
  const { t } = useT()
  if (r.status)
    return (
      <span className="st" data-status={r.status}>
        <span aria-hidden>{r.status}</span><span className="sr-only">{t(`change.${r.status}`)}</span>
      </span>
    )
  if (r.planned) {
    const label = t(r.ghost ? 'tree.ghost' : 'tree.planned')
    return <Tip text={label}><span role="img" aria-label={label} className="st"><Icon name="planned" className="size-3.5" /></span></Tip>
  }
  return <span className="st" />
}

const gone = (id: string): TreeRow =>
  ({ id, name: '', folder: false, children: [], add: 0, del: 0, planned: false, ghost: false, writers: [], changed: false, inPick: false })

function Files({ ft, file, onFile }: { ft: FileTree; file: string | null; onFile: (path: string) => void }) {
  const { t, num } = useT()
  // Open folders start from the pick; the user may open and close freely until it changes.
  const [expanded, setExpanded] = useState(ft.open)
  const openKey = ft.open.join('|')
  useEffect(() => setExpanded(openKey ? openKey.split('|') : []), [openKey])
  const tree = useTree<TreeRow>({
    rootItemId: ROOT,
    state: { expandedItems: expanded, selectedItems: file ? [file] : [] },
    setExpandedItems: setExpanded,
    // The picked file is held above; a click on a file picks it there (or lets it go).
    setSelectedItems: () => {},
    indent: 14,
    getItemName: item => item.getItemData().name,
    isItemFolder: item => item.getItemData().folder,
    // A row can be asked for once more after the tree changed shape: answer with an empty one.
    dataLoader: { getItem: id => ft.rows.get(id) ?? gone(id), getChildren: id => ft.rows.get(id)?.children ?? [] },
    onPrimaryAction: item => { if (!item.isFolder()) onFile(item.getId()) },
    features: [syncDataLoaderFeature, selectionFeature, hotkeysCoreFeature],
  })
  useEffect(() => tree.rebuildTree(), [tree, ft])
  return (
    <div {...tree.getContainerProps(t('tree.label'))} className="ftree">
      {tree.getItems().map(item => {
        const r = item.getItemData()
        if (!r.name) return null
        return (
          <button
            {...item.getProps()}
            key={item.getId()}
            type="button"
            className="trow"
            data-dim={!r.inPick || undefined}
            data-ghost={r.ghost || undefined}
            style={{ paddingLeft: 6 + item.getItemMeta().level * 14 }}
          >
            {r.folder ? <Icon name="chev" className="chev" /> : <Status r={r} />}
            <span className="nm">{r.name}{r.folder && '/'}</span>
            {r.folder && r.planned && !r.add && !r.del && <Status r={{ ...r, folder: false }} />}
            {r.writers.length > 0 && (
              <span className="flex flex-none items-center gap-0.5">
                <Who name={r.writers[0]} live />
                {r.writers.length > 1 && <span className="text-2xs text-ink-2">+{num(r.writers.length - 1)}</span>}
              </span>
            )}
            {(r.add > 0 || r.del > 0) && (
              <span className="ct"><span className="text-add">+{num(r.add)}</span> <span className="text-del">−{num(r.del)}</span></span>
            )}
          </button>
        )
      })}
    </div>
  )
}

/** The picked file whole: every line, its change over the pick (or the whole unit) marked. */
/** The line a row of diff2html's table stands for, by its number now; none for a removed line. */
const lineOf = (row: Element) => Number(row.querySelector('.line-num2')?.textContent?.trim()) || 0

/** What picks lines in a diff, and what is asked about them. */
interface LinePicking {
  lines: Lines | null
  onLines: Dispatch<SetStateAction<Lines | null>>
  /** Ask why about the picked lines: the task that last changed them is picked and its box focused. */
  onAsk: (l: Lines) => void
}

/**
 * A diff whose lines can be picked by their numbers: a click picks one (again: lets it go), a
 * shift-click stretches the pick to it. Next to the picked lines floats "Ask why", like the
 * "+" of a code review.
 */
function Pickable({ path, commit, lines, onLines, onAsk, children }: LinePicking & { path: string; commit?: number; children: ReactNode }) {
  const { t } = useT()
  const wrap = useRef<HTMLDivElement>(null)
  const mine = lines && lines.path === path && lines.commit === commit ? lines : null
  const [top, setTop] = useState<number | null>(null)
  useEffect(() => {
    let first: Element | null = null
    wrap.current?.querySelectorAll('.d2h-diff-tbody tr').forEach(row => {
      const n = lineOf(row)
      const on = !!mine && n > 0 && n >= mine.from && n <= mine.to
      row.toggleAttribute('data-picked', on)
      if (on && !first) first = row
    })
    const box = wrap.current?.getBoundingClientRect()
    setTop(first && box ? (first as Element).getBoundingClientRect().top - box.top : null)
  }, [mine?.from, mine?.to, mine, children])
  const onClick = (e: React.MouseEvent) => {
    const cell = (e.target as HTMLElement).closest('.d2h-code-linenumber')
    const n = cell && lineOf(cell.closest('tr') ?? cell)
    if (!n) return
    onLines(l => {
      const was = l && l.path === path && l.commit === commit ? l : null
      if (e.shiftKey && was) return { path, commit, from: Math.min(was.from, n), to: Math.max(was.to, n) }
      return was && was.from === n && was.to === n ? null : { path, commit, from: n, to: n }
    })
  }
  return (
    <div ref={wrap} className="pickable" onClick={onClick}>
      {children}
      {mine && top !== null && (
        <Button size="xs" className="askfloat rounded-sm" style={{ top }} onClick={e => { e.stopPropagation(); onAsk(mine) }}>
          <Icon name="talk" className="size-3" />{t('ask.why')}
        </Button>
      )}
    </div>
  )
}

interface WholeProps extends LinePicking {
  unit: Unit
  sel: Selection
  path: string
  onPick: (p: Pick) => void
  onBack: () => void
}

function WholeFile({ unit, sel, path, onPick, onBack, ...picking }: WholeProps) {
  const { t } = useT()
  const delta = shownDelta(unit, sel, path)
  const touched = delta?.touched ?? []
  const before = delta?.before ?? null, after = delta?.after ?? null, status = delta?.status ?? 'M'
  // Every line of the file: the context is the whole file.
  const change = useMemo(
    () => ({ path, status, ...changeOf(before, after, Number.MAX_SAFE_INTEGER) }) as Change,
    [path, status, before, after],
  )
  // Which pieces of work the shown change comes from; a commit that names none, by its hash.
  const from = [...new Map(touched.map(k => {
    const c = unit.commits[k], n = unit.tasks.find(x => x.output.commits?.includes(c.id))
    return [n?.id ?? c.id, { node: n, label: n?.taskId ? `#${n.taskId}` : c.live ? t('io.uncommitted') : c.id }] as const
  })).values()]
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-ink-2">
        <Button variant="ghost" size="xs" className="-ml-2 rounded-sm" onClick={onBack}><Icon name="back" className="size-3.5" />{t('file.back')}</Button>
        {from.length > 0 && <span>{t('file.from')}</span>}
        {from.map(({ node, label }) => (
          node?.taskId
            ? <button key={label} type="button" className="chip" data-state={node.state} onClick={() => onPick({ kind: 'task', id: node.id })}>{label}</button>
            : <span key={label} className="font-mono">{label}</span>
        ))}
      </div>
      {touched.length
        ? <Pickable path={path} {...picking}><FileDiff file={change} whole /></Pickable>
        : <p className="text-sm text-ink-2">{t('file.unchanged', { path })}</p>}
    </div>
  )
}

interface Props {
  root: ModuleNode
  unit: Unit
  sel: Selection
  onlyChanged: boolean
  onOnlyChanged: (on: boolean) => void
  onPick: (p: Pick) => void
  /** The picked file and lines in it; held above, since a question takes them as context. */
  file: string | null
  onFile: Dispatch<SetStateAction<string | null>>
  lines: Lines | null
  onLines: Dispatch<SetStateAction<Lines | null>>
  onAsk: (l: Lines) => void
  /** A commit to bring into view (opened from the panel's commits). */
  commit: CommitFocus | null
}

export function CodeLayer({ root, unit, sel, onlyChanged, onOnlyChanged, onPick, file, onFile: setFile, lines, onLines, onAsk, commit }: Props) {
  const { t, date } = useT()
  // Where the tree stacks above the diff, it opens on the pick's files only.
  const narrow = window.matchMedia('(max-width: 720px)').matches
  const ft = useMemo(() => fileTree(root, unit, sel, onlyChanged, narrow), [root, unit, sel, onlyChanged, narrow])
  // ponytail: 30 files per commit in the list; the tree still reaches the rest, one file at a time.
  const limit = 30
  const blocks = unit.commits.flatMap((c, k) => (sel.commits.length && !sel.commits.includes(k) ? [] : c.files.length ? [{ c, k, files: c.files }] : []))
  const waiting = sel.node && !sel.commits.length

  // A commit asked for from the panel: back to the list, and that commit in view.
  useEffect(() => {
    if (!commit) return
    setFile(null)
    const later = setTimeout(() => document.querySelector(`[data-commit="${CSS.escape(commit.id)}"]`)?.scrollIntoView({ block: 'start' }), 60)
    return () => clearTimeout(later)
  }, [commit, setFile])

  // A picked file: kept while the pick still touches it, let go when it does not.
  useEffect(() => setFile(null), [unit.id, setFile])
  useEffect(() => setFile(f => (f && ft.rows.get(f)?.inPick ? f : null)), [ft, setFile])
  // Escape lets the file go first; only the next one reaches the pick.
  useEffect(() => {
    if (!file) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return
      e.stopPropagation()
      setFile(null)
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [file, setFile])

  return (
    <section className="block-box">
      <h2 className="mb-3 text-lg font-[650]">{t('block.code')}</h2>
      <div className="code-grid">
        {/* The tree keeps its place while the changes scroll past it; its switch goes with it. */}
        <div className="code-side">
          <label className="flex items-center gap-2 px-1.5 pb-2 text-xs">
            <Switch size="sm" checked={onlyChanged} onCheckedChange={onOnlyChanged} />
            {t('tree.onlyChanged')}
          </label>
          <ScrollArea className="code-tree">
            {ft.rows.get(ROOT)!.children.length
              ? <Files key={`${unit.id}:${onlyChanged}`} ft={ft} file={file} onFile={p => setFile(f => (f === p ? null : p))} />
              : <p className="p-2 text-sm text-ink-2">{t('code.none')}</p>}
          </ScrollArea>
        </div>
        <div className="min-w-0">
          {file ? (
            <WholeFile unit={unit} sel={sel} path={file} onPick={onPick} onBack={() => setFile(null)} lines={lines} onLines={onLines} onAsk={onAsk} />
          ) : waiting ? (
            <div className="text-sm wrap-anywhere">
              {t('io.notStarted')}
              {sel.plans && sel.plans.length > 0 && (
                <div className="font-mono text-xs text-ink-2">{t('io.willChange', { paths: [...new Set(sel.plans)].join(t('list.sep')) })}</div>
              )}
            </div>
          ) : blocks.length ? (
            blocks.map(({ c, k, files }) => (
              <div key={c.id} className="round" data-commit={c.id}>
                <div className="text-sm font-semibold wrap-anywhere">{c.live ? t('io.uncommitted') : c.subject}</div>
                <div className="mb-2 font-mono text-xs text-ink-2">{[c.live ? '' : c.id, date(c.date), c.author].filter(Boolean).join(' · ')}</div>
                <div className="flex flex-col gap-2">
                  {files.slice(0, limit).map(f => <Pickable key={f.path} path={f.path} commit={k} lines={lines} onLines={onLines} onAsk={onAsk}><FileDiff file={f} /></Pickable>)}
                  {files.length > limit && <div className="text-xs text-ink-2">{t('code.moreFiles', { count: files.length - limit })}</div>}
                </div>
              </div>
            ))
          ) : (
            <p className="text-sm text-ink-2">{t('code.none')}</p>
          )}
        </div>
      </div>
    </section>
  )
}
