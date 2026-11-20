// The page acts as the user, so it works only with a session, which a one-time link from
// the user's own terminal gives (`workriver open`: the code in the address's fragment is
// exchanged for a cookie the page itself cannot read, and taken out of the address). Without
// one, nothing of the app is shown: only how to open it.
import { useEffect, useState, type ReactNode } from 'react'
import { Waves } from 'lucide-react'
import { MOCK } from '@/workspace/source'
import { useT } from '@/i18n'
import { rememberWanted, SIGNED_OUT, startSession } from '@/runtime'
import { Steps } from './bits'

export function Session({ children }: { children: ReactNode }) {
  const { t } = useT()
  // null: not known yet; 'down': the service does not answer.
  const [user, setUser] = useState<boolean | 'down' | null>(MOCK ? true : null)
  useEffect(() => {
    if (MOCK) return
    const check = () => startSession().then(setUser, () => setUser('down'))
    check()
    // A session that ended (a new token, 30 days) shows on the next thing the user does.
    window.addEventListener(SIGNED_OUT, check)
    // The link opened in a tab that already shows this page changes only the fragment.
    window.addEventListener('hashchange', check)
    return () => { window.removeEventListener(SIGNED_OUT, check); window.removeEventListener('hashchange', check) }
  }, [])
  // Signed out: a link to a unit or task opened here is where the sign-in (`workriver open`,
  // in a new tab) lands; and this tab follows once it is back in front.
  useEffect(() => {
    if (user !== false) return
    rememberWanted()
    const again = () => startSession().then(setUser, () => setUser('down'))
    window.addEventListener('focus', again)
    return () => window.removeEventListener('focus', again)
  }, [user])
  if (user === true) return children
  if (user === null) return null
  return (
    <main className="mx-auto flex min-h-screen max-w-[60ch] flex-col justify-center gap-3 p-6">
      <h1 className="flex items-center gap-2 text-xl font-[650] tracking-[-0.012em]"><Waves aria-hidden size={20} strokeWidth={1.6} />{t('session.title')}</h1>
      {user === 'down'
        ? <p role="status" className="text-sm text-ink-2">{t('service.unreachable')}</p>
        : (
          <>
            <p className="text-sm text-ink-2">{t('session.lead')}</p>
            <Steps steps={[{ text: t('session.open'), command: 'workriver open' }]} />
            <p className="text-sm text-ink-2">{t('session.why')}</p>
          </>
        )}
    </main>
  )
}
