import { useState } from 'react'
import { closeWork } from '@/api/actions'
import { Content } from '@/content/Chips'
import { useT } from '@/i18n'
import { Button } from '@/ui/primitives/button'
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from '@/ui/primitives/dialog'
import { Input } from '@/ui/primitives/input'

export function ClosedReason({ reason }: { reason?: string }) {
  const { t } = useT()
  return reason && <p className="mt-0.5 max-w-[75ch] wrap-anywhere text-sm text-ink-2">
    <span className="mr-1.5 font-medium">{t('closeWork.reason')}</span><Content text={reason} />
  </p>
}

/** A successful close may still have a warning to read after the workspace updates. */
export function CloseDialog({ kind, id, available, advice }: {
  kind: 'unit' | 'intent'; id: string; available: boolean; advice?: string
}) {
  const { t } = useT()
  const [open, setOpen] = useState(false)
  const [reason, setReason] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [warning, setWarning] = useState('')
  const valid = !!reason.trim() && !/[\r\n\v\f\u0085\u2028\u2029]/.test(reason)
  const action = t(`closeWork.action.${kind}`)
  const save = async () => {
    if (!valid || busy) return
    setBusy(true)
    setError('')
    try {
      const { warning } = await closeWork(kind, id, reason.trim())
      if (warning) setWarning(warning)
      else setOpen(false)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog open={open} onOpenChange={next => {
      if (busy) return
      setOpen(next)
      if (next) { setReason(''); setError('') }
      else setWarning('')
    }}>
      {available && <DialogTrigger asChild><Button variant="outline" size="sm" className="flex-none">{action}</Button></DialogTrigger>}
      <DialogContent closeLabel={t('settings.close')} showCloseButton={false} className="max-h-[calc(100dvh-2rem)] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{warning ? t('closeWork.closed', { id }) : t(`closeWork.title.${kind}`, { id })}</DialogTitle>
          <DialogDescription className="wrap-anywhere">{warning || (advice ?? t('closeWork.hint'))}</DialogDescription>
        </DialogHeader>
        {warning ? (
          <div className="flex justify-end">
            <DialogClose asChild><Button autoFocus size="sm">{t('settings.close')}</Button></DialogClose>
          </div>
        ) : (
          <form className="flex flex-col gap-3" onSubmit={e => { e.preventDefault(); void save() }}>
            <label className="flex flex-col gap-1 text-sm">
              <span className="font-[550]">{t('closeWork.reason')}</span>
              <Input autoFocus required value={reason} disabled={busy} onChange={e => { setReason(e.target.value); setError('') }} />
            </label>
            {error && <p role="alert" className="text-xs wrap-anywhere text-del">{error}</p>}
            <div className="flex justify-end gap-2">
              <DialogClose asChild><Button type="button" variant="ghost" size="sm" disabled={busy}>{t('rail.addCancel')}</Button></DialogClose>
              <Button type="submit" size="sm" disabled={busy || !valid}>{action}</Button>
            </div>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}
