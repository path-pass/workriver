// The attachment viewer: one place's attachments (pictures, Markdown, plain text) one at a
// time, with previous / next over them. Not modal: the side panel stays usable beside it, so
// the user can ask about what they are looking at; while it shows a stored attachment, the
// ask box offers that attachment as context.
import { createContext, lazy, Suspense, useContext, useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from 'react'
import { ChevronLeft, ChevronRight, Download, ExternalLink, ZoomIn, ZoomOut } from 'lucide-react'
import { Button } from '@/ui/primitives/button'
import { Dialog, DialogContent, DialogTitle } from '@/ui/primitives/dialog'
import { contentLang, useT } from '@/i18n'
import type { Attachment } from './types'
import { isField } from '@/lib/isField'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { kindOf } from './preview'
import { CAP } from '@/config/defaults'

/** react-markdown and remark-gfm load only once a Markdown file is opened. */
const MarkdownView = lazy(() => import('@/content/Markdown'))

/** What the shell does for the viewer: hears which stored attachment is shown (the ask box
 *  offers it), opens Ask, and calls registered close handlers on in-place navigation. */
export const ViewerContext = createContext<{
  onView: (a: Attachment | null) => void
  onAsk: () => void
  registerClose: (close: () => void) => () => void
} | null>(null)

/** The first `cap` bytes of a file as text, and its whole size; null while it comes. */
function useText(url: string, cap: number) {
  const [got, setGot] = useState<{ url: string; text: string; size: number } | { url: string; failed: true } | null>(null)
  useEffect(() => {
    let gone = false
    fetch(url)
      .then(r => { if (!r.ok) throw new Error(String(r.status)); return r.blob() })
      .then(async b => { const text = await b.slice(0, cap).text(); if (!gone) setGot({ url, text, size: b.size }) })
      .catch(() => { if (!gone) setGot({ url, failed: true }) })
    return () => { gone = true }
  }, [url, cap])
  return got?.url === url ? got : null
}

function TextBody({ a, kind }: { a: Attachment; kind: 'markdown' | 'text' }) {
  const { t, lang } = useT()
  const got = useText(a.url, CAP[kind])
  const loading = <p className="text-sm text-ink-2" aria-live="polite">{t('attach.loading')}</p>
  if (!got) return loading
  if ('failed' in got) return <p className="text-sm text-del" role="alert">{t('attach.failed')}</p>
  const cut = got.size > CAP[kind]
  const size = (b: number) => new Intl.NumberFormat(lang, { style: 'unit', unit: b >= 1024 ** 2 ? 'megabyte' : 'kilobyte', maximumFractionDigits: 1 }).format(b >= 1024 ** 2 ? b / 1024 ** 2 : b / 1024)
  // Whole JSON reads better indented.
  let text = got.text
  if (kind === 'text' && !cut && /json$/i.test(a.type ?? a.name)) try { text = JSON.stringify(JSON.parse(text), null, 2) } catch { /* shown as it is */ }
  return (
    <>
      {cut && <p className="mb-3 text-xs text-ink-2">{t('attach.cut', { shown: size(CAP[kind]), size: size(got.size) })}</p>}
      {kind === 'markdown' ? <Suspense fallback={loading}><MarkdownView text={text} /></Suspense> : <pre className="viewer-text">{text}</pre>}
    </>
  )
}

/**
 * `list` are the attachments that preview, in their order; `at` the one shown. `stored`: they
 * are in the service's store (not pictures still being put in a form), so they can be asked about.
 */
export function Viewer({ list, at, onAt, onClose, stored }: { list: Attachment[]; at: number; onAt: (k: number) => void; onClose: () => void; stored: boolean }) {
  const { t } = useT()
  const shell = useContext(ViewerContext)
  const latest = useRef(shell)
  latest.current = shell
  useEffect(() => shell?.registerClose(onClose), [shell, onClose])
  const a = list[at]
  const kind = kindOf(a)
  // Whole picture, or as wide as the viewer with the rest scrolled: a tall screenshot fitted
  // whole is too small to read. Each picture starts whole.
  const [wide, setWide] = useState(false)
  useEffect(() => setWide(false), [a.id])
  // The ask box offers what is shown, while it is shown.
  useEffect(() => {
    if (!stored) return
    latest.current?.onView(a)
    return () => latest.current?.onView(null)
  }, [a, stored])
  const content = useRef<HTMLDivElement>(null)
  const body = useRef<HTMLDivElement>(null)
  const opener = useRef<HTMLElement | null>(null)
  const many = list.length > 1
  const go = (k: number) => onAt((k + list.length) % list.length)
  // ← and → anywhere in the viewer but a field; ↑ ↓ and the page keys scroll the file.
  const onKeyDown = (e: KeyboardEvent) => {
    if (!many || isField(e.target) || e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return
    if (e.key === 'ArrowLeft') { e.preventDefault(); go(at - 1) }
    if (e.key === 'ArrowRight') { e.preventDefault(); go(at + 1) }
  }
  const tool = (label: string, icon: ReactNode, onClick: () => void, pressed?: boolean) => (
    <Tip text={label}>
      <Button variant="ghost" size="icon-sm" aria-label={label} aria-pressed={pressed} onClick={onClick}>{icon}</Button>
    </Tip>
  )

  return (
    <Dialog open modal={false} onOpenChange={o => { if (!o) onClose() }}>
      <DialogContent
        ref={content}
        closeLabel={t('settings.close')}
        aria-describedby={undefined}
        // The file takes the keys at once: ↑ ↓ scroll it, ← → switch.
        onOpenAutoFocus={e => { e.preventDefault(); opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null; body.current?.focus() }}
        // Closed, focus goes back to the thumbnail or chip that opened it.
        onCloseAutoFocus={e => { if (opener.current?.isConnected) { e.preventDefault(); opener.current.focus() } }}
        // The page around stays in use (the side panel, Ask): a click there leaves it open,
        // and Escape typed in a field there is that field's.
        onInteractOutside={e => e.preventDefault()}
        onEscapeKeyDown={e => { const at = document.activeElement; if (isField(at) && !content.current?.contains(at)) e.preventDefault() }}
        onKeyDown={onKeyDown}
        className="inset-4 right-[calc(var(--side-now,2.75rem)+1rem)] flex w-auto max-w-none translate-x-0 translate-y-0 flex-col gap-2 p-3 shadow-xl sm:max-w-none max-[860px]:inset-0 max-[860px]:rounded-none"
      >
        <div className="flex min-w-0 flex-wrap items-center gap-1 pr-8">
          <DialogTitle className="min-w-0 flex-1 truncate font-mono text-sm max-[640px]:basis-full" title={a.name} lang={contentLang(a.name)}>{a.name}</DialogTitle>
          {many && (
            <>
              {tool(t('attach.prev'), <ChevronLeft aria-hidden />, () => go(at - 1))}
              <span aria-hidden className="min-w-10 text-center text-xs text-ink-2 tabular-nums">{t('attach.counter', { n: at + 1, count: list.length })}</span>
              <span className="sr-only" aria-live="polite">{t('attach.now', { n: at + 1, count: list.length, name: a.name })}</span>
              {tool(t('attach.next'), <ChevronRight aria-hidden />, () => go(at + 1))}
            </>
          )}
          {kind === 'image' && tool(wide ? t('attach.zoomOut') : t('attach.zoomIn'), wide ? <ZoomOut aria-hidden /> : <ZoomIn aria-hidden />, () => setWide(w => !w), wide)}
          {stored && shell && (
            <Tip text={t('attach.ask')}>
              <Button variant="ghost" size="xs" className="rounded-sm" onClick={shell.onAsk}><Icon name="talk" className="size-3.5" />{t('talk.ask')}</Button>
            </Tip>
          )}
          <Tip text={t('attach.download')}>
            <Button variant="ghost" size="icon-sm" asChild>
              <a href={a.url} download={a.name} aria-label={t('attach.download')}><Download aria-hidden /></a>
            </Button>
          </Tip>
          {kind === 'image' && (
            <Tip text={t('attach.original')}>
              <Button variant="ghost" size="icon-sm" asChild>
                <a href={a.url} target="_blank" rel="noopener noreferrer" aria-label={t('attach.original')}><ExternalLink aria-hidden /></a>
              </Button>
            </Tip>
          )}
        </div>
        <div ref={body} tabIndex={0} aria-label={a.name} className="min-h-0 flex-1 overflow-auto overscroll-contain rounded-md outline-none focus-visible:ring-2 focus-visible:ring-you">
          {kind === 'image' ? (
            <img
              src={a.url}
              alt={a.name}
              onClick={() => setWide(w => !w)}
              className={wide ? 'h-auto w-full max-w-none cursor-zoom-out' : 'h-full w-full cursor-zoom-in object-contain'}
            />
          ) : kind ? <div className="p-1"><TextBody key={a.id} a={a} kind={kind} /></div> : null}
        </div>
      </DialogContent>
    </Dialog>
  )
}
