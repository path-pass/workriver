// Editing an intent's title, goal and note, in a dialog opened from its page (the pencil in
// its header, or "Restore this version" in its history). Save records the fields that
// changed against the intent as the dialog opened it, with that version as the base, however
// the page refreshes meanwhile; when one of them was changed since, the service refuses it,
// and the dialog shows every field it sends that changed since, each against the user's text,
// with "Save anyway". The page is the user's, so every field can be edited here.
import { useState } from 'react'
import { Button } from '@/ui/primitives/button'
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/ui/primitives/dialog'
import { Input } from '@/ui/primitives/input'
import { ToggleGroup, ToggleGroupItem } from '@/ui/primitives/toggle-group'
import { contentLang, useT } from '@/i18n'
import { Prose } from '@/content/Prose'
import { editIntent, intentVersion } from '@/api/actions'
import { conflictOf, conflictsOf, draftOf, editBody, wordDiff, type Draft, type Field } from './edit'
import { Runs } from './IntentHistory'
import type { Intent } from './types'

/** What changed since the dialog opened: the version the intent is at, and each field the
 *  edit sends that changed since, with its text now. */
interface Conflict {
  now: number;
  fields: { field: Field; theirs: string }[];
}

export function EditIntentDialog({ intent, draft, restoring, onClose }: {
  intent: Intent
  /** What the fields start with: the intent's text, or a version to restore. */
  draft: Draft
  /** The version being restored, if any. */
  restoring?: number
  onClose: () => void
}) {
  const { t } = useT()
  const [d, setD] = useState<Draft>(draft)
  const [preview, setPreview] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [conflict, setConflict] = useState<Conflict | null>(null)
  // The intent as the dialog opened it: what the edit changes, and the version it is made
  // from, whatever the page shows meanwhile.
  const [opened] = useState(intent)
  const body = editBody(opened, d)
  const ready = !!body && !busy
  const set = (k: keyof Draft) => (e: { target: { value: string } }) => setD(x => ({ ...x, [k]: e.target.value }))

  const save = (anyway: boolean) => {
    if (!body || busy) return
    setBusy(true)
    setError('')
    editIntent(intent, anyway && conflict ? { ...body, base: conflict.now } : body).then(
      onClose,
      async (e: unknown) => {
        const said = String(e instanceof Error ? e.message : e)
        const c = conflictOf(said)
        if (c) {
          // What it says now: every field this edit sends that changed since it opened (the
          // service names only the first), to set against the user's text.
          const now = await intentVersion(intent, c.now).catch(() => null)
          if (now) {
            const changed = conflictsOf(draftOf(opened), now.text, body)
            const fields = (changed.length ? changed : [c.field]).map(field => ({ field, theirs: now.text[field] }))
            setConflict({ now: c.now, fields })
            setBusy(false)
            return
          }
        }
        setConflict(null)
        setError(t('intent.edit.failed', { error: said }))
        setBusy(false)
      },
    )
  }

  return (
    <Dialog open onOpenChange={o => { if (!o) onClose() }}>
      <DialogContent closeLabel={t('settings.close')} className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t('intent.edit.title')} <span className="font-mono text-sm font-normal text-ink-2">{intent.id}</span></DialogTitle>
          <DialogDescription className="text-xs text-ink-2">{restoring ? t('intent.edit.restoring', { n: restoring }) : t('intent.edit.hint')}</DialogDescription>
        </DialogHeader>
        <form className="flex min-w-0 flex-col gap-3" onSubmit={e => { e.preventDefault(); save(false) }}>
          <label className="flex flex-col gap-1 text-xs text-ink-2">
            {t('intent.edit.fieldTitle')}
            <Input value={d.title} lang={contentLang(d.title)} onChange={set('title')} aria-invalid={!d.title.trim() || undefined} className="h-8 text-sm text-ink md:text-sm" />
          </label>
          <label className="flex flex-col gap-1 text-xs text-ink-2">
            {t('intent.edit.fieldGoal')}
            <textarea className="field text-sm text-ink" rows={2} value={d.goal} lang={contentLang(d.goal)} onChange={set('goal')} />
          </label>
          <div className="flex flex-col gap-1">
            <div className="flex items-center justify-between gap-2 text-xs text-ink-2">
              <span id="edit-note">{t('intent.edit.fieldNote')}</span>
              <ToggleGroup type="single" variant="outline" size="sm" value={preview ? 'preview' : 'write'} onValueChange={v => { if (v) setPreview(v === 'preview') }}>
                <ToggleGroupItem value="write" className="h-6 px-2 text-xs data-[state=on]:font-semibold">{t('intent.edit.write')}</ToggleGroupItem>
                <ToggleGroupItem value="preview" className="h-6 px-2 text-xs data-[state=on]:font-semibold">{t('intent.edit.preview')}</ToggleGroupItem>
              </ToggleGroup>
            </div>
            {preview
              ? <div className="min-h-32 rounded-lg border border-line-2 px-2.5 py-1.5 text-sm" aria-labelledby="edit-note">{d.note.trim() ? <Prose text={d.note} fold={false} /> : <p className="text-ink-2">{t('intent.edit.nothing')}</p>}</div>
              : <textarea className="field text-sm" rows={8} value={d.note} lang={contentLang(d.note)} onChange={set('note')} aria-labelledby="edit-note" />}
          </div>
          {conflict && (
            <div role="alert" className="flex flex-col gap-1.5 rounded-lg border border-line-2 bg-sunken p-2.5 text-xs">
              {conflict.fields.map(({ field, theirs }) => (
                <div key={field} className="flex flex-col gap-1">
                  <p>{t(`intent.edit.conflict.${field}`, { n: conflict.now })}</p>
                  <p className="max-h-48 overflow-y-auto whitespace-pre-wrap text-sm wrap-anywhere" lang={contentLang(theirs)}>
                    <Runs runs={wordDiff(theirs, field === 'note' ? d.note : d[field].trim())} />
                  </p>
                </div>
              ))}
              <p className="text-ink-2">{t('intent.edit.anyway')}</p>
            </div>
          )}
          {error && <p role="alert" className="text-xs text-del wrap-anywhere">{error}</p>}
          <div className="flex justify-end gap-2">
            <DialogClose asChild><Button type="button" variant="ghost" size="sm">{t('intent.edit.cancel')}</Button></DialogClose>
            {conflict
              ? <Button type="button" size="sm" disabled={!ready} onClick={() => save(true)}>{t('intent.edit.saveAnyway')}</Button>
              : <Button type="submit" size="sm" disabled={!ready}>{t('intent.edit.save')}</Button>}
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
