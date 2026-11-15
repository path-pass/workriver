// What to do first, shown in the main area while nothing is recorded yet.
import { useT } from '@/i18n'
import { Steps } from './bits'

export function Start() {
  const { t } = useT()
  return (
    <section aria-labelledby="start" className="flex max-w-[70ch] flex-col gap-3 pt-2">
      <h1 id="start" className="text-xl font-[650] tracking-[-0.012em]">{t('start.title')}</h1>
      <p className="text-sm text-ink-2">{t('start.lead')}</p>
      <Steps
        steps={[
          { text: t('start.agent') },
          { text: t('start.skill'), command: 'workriver guide manager' },
          { text: t('start.register'), command: 'workriver manager here --goal "…"' },
          { text: t('start.work') },
        ]}
      />
      <p className="text-sm text-ink-2">{t('start.trailer')} <code className="font-mono text-xs">Task: u1#3</code></p>
    </section>
  )
}
