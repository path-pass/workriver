// A place's attachments as thumbnails and file chips, each opening the viewer.
import { useState } from 'react'
import { useT } from '@/i18n'
import type { Attachment } from './types'
import { ChipName } from '@/content/Chips'
import { Icon } from '@/ui/Icon'
import { Viewer } from './Viewer'
import { kindOf } from './preview'

/** Small pictures and other files as chips. A click opens the viewer over every one of them
 *  that previews (pictures, Markdown, plain text), in their order; any other file downloads.
 *  With `onRemove` (pictures still being put in a form), each has a ×. */
export function Thumbs({ list, onRemove }: { list: Attachment[]; onRemove?: (a: Attachment) => void }) {
  const { t } = useT()
  const [open, setOpen] = useState<number | null>(null)
  if (!list.length) return null
  const shown = list.filter(a => kindOf(a))
  const view = (a: Attachment) => setOpen(shown.indexOf(a))
  const files = list.filter(a => kindOf(a) !== 'image')
  const images = list.filter(a => kindOf(a) === 'image')
  return (
    <>
      {files.length > 0 && (
        <div className="mt-1 flex flex-wrap gap-1">
          {files.map(a => kindOf(a)
            ? <button key={a.id} type="button" className="ctx font-mono text-xs hover:text-ink" onClick={() => view(a)}><Icon name="file" className="size-3" /><ChipName name={a.name} /></button>
            : <a key={a.id} href={a.url} download={a.name} className="ctx font-mono text-xs"><Icon name="attach" className="size-3" /><ChipName name={a.name} /></a>)}
        </div>
      )}
      {images.length > 0 && <div className="thumbs">
        {images.map(a => (
          <span key={a.id} className="thumb">
            <button type="button" aria-label={t('attach.open', { name: a.name })} onClick={() => view(a)}><img src={a.url} alt="" /></button>
            {onRemove && <button type="button" className="x" aria-label={t('attach.remove', { name: a.name })} onClick={() => onRemove(a)}><Icon name="no" className="size-3" /></button>}
          </span>
        ))}
      </div>}
      {open !== null && shown[open] && <Viewer list={shown} at={open} onAt={setOpen} onClose={() => setOpen(null)} stored={!onRemove} />}
    </>
  )
}
