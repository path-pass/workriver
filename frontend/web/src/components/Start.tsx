// What to do first, shown in the main area while nothing is recorded yet; and the steps to
// start one planned intent, which workriver cannot do itself (it never starts agents).
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import { useT } from '@/i18n'
import type { Intent } from '@/types'
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
          { text: t('start.register'), command: 'workriver manager here' },
          { text: t('start.begin'), command: 'workriver backlog list' },
          { text: t('start.work') },
        ]}
      />
      <p className="text-sm text-ink-2">{t('start.trailer')} <code className="font-mono text-xs">Task: u1#3</code></p>
      <Steps steps={[{ text: t('start.import'), command: 'workriver guide import' }]} />
    </section>
  )
}

/** The steps to start a planned intent in a Herdr workspace, each command with a copy button. */
export function StartIntent({ intent, onClose }: { intent: Intent | null; onClose: () => void }) {
  const { t } = useT()
  return (
    <Dialog open={!!intent} onOpenChange={o => { if (!o) onClose() }}>
      <DialogContent closeLabel={t('settings.close')}>
        {intent && (
          <>
            <DialogTitle>{t('plan.title', { title: intent.title })}</DialogTitle>
            <DialogDescription>{t('plan.lead')}</DialogDescription>
            <Steps
              steps={[
                { text: t('plan.workspace') },
                { text: t('plan.agent'), command: 'workriver guide manager' },
                { text: t('plan.register'), command: 'workriver manager here' },
                { text: t('plan.start'), command: `workriver intent start ${intent.id}` },
              ]}
            />
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}
