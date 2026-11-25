import { replyPath, type ReplyPlace } from '@/app/link'
import { Prose } from '@/content/Prose'
import { useT } from '@/i18n'
import { repliesOf } from './select'
import type { ManagerMessage } from './types'

export function ReplyPage({ messages, place, down }: { messages: ManagerMessage[] | null; place: ReplyPlace; down: boolean }) {
  const { t, when } = useT()
  const message = messages?.find(m => m.id === place.message)
  const entry = message && (place.reply === 0
    ? message.byManager && { by: message.manager, text: message.text, at: message.at }
    : repliesOf(message).find((r, i) => (r.reply ?? i + 1) === place.reply))
  return (
    <main className="mx-auto flex max-w-[75ch] flex-col gap-4 px-6 pt-5 pb-10 max-[860px]:p-4">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-xl font-[650]">{t('dashboard.reply')}</h1>
        <a href="/" className="text-sm underline underline-offset-2">{t('dashboard.back')}</a>
      </header>
      {entry ? (
        <article className="flex flex-col gap-3 rounded-lg border border-line p-4 text-sm">
          <p className="flex flex-wrap gap-x-3 text-ink-2"><strong>{entry.by}</strong><time dateTime={entry.at}>{when(Date.parse(entry.at))}</time></p>
          <Prose text={entry.text} fold={false} />
        </article>
      ) : (
        <p role="status" className="text-sm text-ink-2">
          {down ? t('service.unreachable') : messages ? t('link.notFound', { ref: replyPath(place).slice(1) }) : t('settings.loading')}
        </p>
      )}
    </main>
  )
}
