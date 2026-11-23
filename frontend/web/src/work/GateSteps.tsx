// A unit's steps placed from its template, as a task's detail shows them: on a placed task,
// which step it is and what kind of step; on every gate of such a unit, what came of the steps
// (done by whom, left out with who and why, not done). The service works both out; the page
// computes nothing.
import type { GateSteps as Listed, StepMark, Task } from './types'
import type { Unit } from '@/planning/types'
import { contentLang, useT } from '@/i18n'
import { Icon } from '@/ui/Icon'
import { openTemplates } from '@/planning/Templates'

/** The template's name, as a link to it in Templates. */
function TemplateName({ name }: { name: string }) {
  return <button type="button" className="font-mono underline decoration-line underline-offset-2 hover:decoration-ink-2" onClick={() => openTemplates(name)}>{name}</button>
}

/** A placed task: which step of which template, and what kind of step it is (the template's
 *  words: the user's, or workriver's for a built-in); not filled yet, who fills it. */
export function StepLine({ unit, n }: { unit: Unit; n: Task }) {
  const { t } = useT()
  if (!n.step) return null
  const template = unit.template
  return (
    <div className="mb-2 flex flex-col gap-0.5 border-l-2 border-line pl-2.5 text-xs" aria-label={t('template.stepLabel')}>
      <span className="flex flex-wrap items-center gap-1 text-ink-2">
        <Icon name="template" className="size-3.5" />
        {t('template.step', { id: n.step.id })}
        {template && <>· <TemplateName name={template.name} /> ({t(template.builtin ? 'template.builtin' : 'template.yours')})</>}
      </span>
      <span className="whitespace-pre-line" lang={contentLang(n.step.description)}>{n.step.description}</span>
      {!n.step.filled && n.state === 'todo' && <span className="text-ink-2">{t('template.toFill')}</span>}
    </div>
  )
}

/** What a gate lists of its unit's placed steps, under the template's name. */
export function GateStepsList({ steps }: { steps: Listed }) {
  const { t } = useT()
  const by = (m: StepMark) => (m.by === 'user' ? t('template.byYou') : m.by ? t('template.by', { by: m.by }) : '')
  const step = (m: StepMark) => `${m.id} (#${m.n})`
  return (
    <section className="flex flex-col gap-1 rounded-sm border border-line px-2.5 py-2 text-xs" aria-label={t('template.listLabel', { name: steps.template })}>
      <h4 className="flex items-center gap-1 font-semibold text-ink-2">
        <Icon name="template" className="size-3.5" />
        {t('template.list')} <TemplateName name={steps.template} /> ({t(steps.users ? 'template.yours' : 'template.builtin')})
      </h4>
      {steps.done.length > 0 && (
        <p><span className="font-semibold">{t('template.done')}</span> {steps.done.map(m => `${step(m)}${by(m) && `, ${by(m)}`}`).join(t('list.sep'))}</p>
      )}
      {steps.leftOut.map(m => (
        <p key={m.n} data-left-out>
          <span className="font-semibold">{t('template.leftOut')}</span> {step(m)}{by(m) && `, ${by(m)}`}:{' '}
          <span className="whitespace-pre-line" lang={contentLang(m.reason ?? '')}>{m.reason}</span>
        </p>
      ))}
      {steps.notDone.length > 0 && (
        <p data-not-done><span className="font-semibold">{t('template.notDone')}</span> {steps.notDone.map(step).join(t('list.sep'))}</p>
      )}
      {!steps.done.length && !steps.leftOut.length && !steps.notDone.length && <p className="text-ink-2">{t('template.nothingBefore')}</p>}
    </section>
  )
}
