// Adding a project, the user's act: a folder of the machine workriver runs on, typed whole
// (no folder browser: a browser gives a page no real path), and a title. It comes with its
// first workspace, whose line in the rail then offers "Start manager".
import { useEffect, useState } from 'react'
import { Button } from '@/ui/primitives/button'
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogTitle } from '@/ui/primitives/dialog'
import { Input } from '@/ui/primitives/input'
import { useT } from '@/i18n'
import { addProject } from '@/api/actions'
import { Icon } from '@/ui/Icon'
import { cn } from '@/lib/utils'

/** A folder given whole: from `/`, or from `~/` (the service's home). The service holds the
 *  same rule; the page says it before sending, as a convenience. */
export const wholePath = (p: string) => p.startsWith('/') || p.startsWith('~/')

/** Each add-project dialog on the page. */
const openers = new Set<() => void>()
/** Opens the dialog that adds a project. */
export function openAddProject() { openers.forEach(open => open()) }

/** "Add project", wherever it is offered: the rail's foot of the projects, the start guide,
 *  the Start dialog with no workspace. */
export function AddProjectButton({ tone = 'page', className }: { tone?: 'rail' | 'page'; className?: string }) {
  const { t } = useT()
  return (
    <Button
      type="button" size="sm" variant={tone === 'rail' ? 'ghost' : 'outline'} onClick={openAddProject}
      className={cn(tone === 'rail' && 'h-7 justify-start gap-1.5 rounded-sm px-1.5 text-xs font-normal text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink', className)}
    >
      <Icon name="plus" className="size-3.5" />{t('project.add')}
    </Button>
  )
}

/** The one add-project dialog of the page: the folder and an optional title; a refusal in the
 *  service's words. On success it closes and the page shows the project as it reads it again. */
export function AddProjectDialog() {
  const { t } = useT()
  const [open, setOpen] = useState(false)
  const [path, setPath] = useState('')
  const [title, setTitle] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    const opener = () => { setPath(''); setTitle(''); setError(''); setBusy(false); setOpen(true) }
    openers.add(opener)
    return () => { openers.delete(opener) }
  }, [])
  const save = () => {
    const folder = path.trim()
    if (!folder) return
    if (!wholePath(folder)) return setError(t('project.whole'))
    setBusy(true)
    addProject(folder, title.trim()).then(
      () => { setBusy(false); setOpen(false) },
      e => { setBusy(false); setError(t('project.failed', { error: e instanceof Error ? e.message : String(e) })) },
    )
  }
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent closeLabel={t('settings.close')}>
        <DialogTitle>{t('project.title')}</DialogTitle>
        <DialogDescription>{t('project.lead')}</DialogDescription>
        <form className="flex flex-col gap-3" onSubmit={e => { e.preventDefault(); save() }}>
          <label className="flex flex-col gap-1 text-sm">
            <span className="font-[550]">{t('project.folder')}</span>
            <Input
              autoFocus value={path} onChange={e => { setPath(e.target.value); setError('') }} placeholder={t('project.folderPlaceholder')}
              spellCheck={false} autoCapitalize="off" autoCorrect="off" aria-invalid={!!error || undefined} className="h-8 font-mono text-sm md:text-sm"
            />
            <span className="text-xs text-ink-2">{t('project.folderHint')}</span>
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="font-[550]">{t('project.name')}</span>
            <Input value={title} onChange={e => setTitle(e.target.value)} placeholder={t('project.titlePlaceholder')} className="h-8 text-sm md:text-sm" />
          </label>
          {error && <p role="alert" className="text-xs wrap-anywhere text-del">{error}</p>}
          <div className="flex justify-end gap-2">
            <DialogClose asChild><Button type="button" variant="ghost" size="sm">{t('rail.addCancel')}</Button></DialogClose>
            <Button type="submit" size="sm" disabled={busy || !path.trim()}>{t('project.add')}</Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
