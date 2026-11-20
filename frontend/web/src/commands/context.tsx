// The steps an agent prepared for the user to do by hand, and how a step's command asks to
// run: the shell supplies RunContext; RunSteps hands it each step.
import { Button } from '@/ui/primitives/button'
import { contentLang, useT } from '@/i18n'
import type { Step } from '@/work/types'
import { Tip } from '@/ui/Tip'
import { Icon } from '@/ui/Icon'
import { CopyButton } from '@/ui/CopyButton'
import { createContext, useContext } from 'react'
import type { RunAsk } from './types'

/**
 * What an agent prepared for the user to do by hand: the same for a gate and for a chore.
 * With `onRun`, each command also has "Run", which asks to run it as the user.
 */
export function Steps({ steps, onRun }: { steps: Step[]; onRun?: (command: string, step: number) => void }) {
  const { t } = useT()
  return (
    <ol className="steps">
      {steps.map((s, k) => (
        <li key={k}>
          <span className="whitespace-pre-line" lang={contentLang(s.text)}>{s.text}</span>
          {s.command && (
            <div className="cmd">
              <code>{s.command}</code>
              <CopyButton text={s.command} />
              {onRun && (
                <Tip text={t('run.buttonFor', { command: s.command })}>
                  <Button variant="ghost" size="xs" className="flex-none rounded-sm" aria-label={t('run.buttonFor', { command: s.command })} onClick={() => onRun(s.command!, k + 1)}>
                    <Icon name="play" className="size-3" />{t('run.button')}
                  </Button>
                </Tip>
              )}
            </div>
          )}
        </li>
      ))}
    </ol>
  )
}

/** Asks to run a command (opens the confirm dialog); App provides it. */
export const RunContext = createContext<(a: RunAsk) => void>(() => {})

/** A gate's or chore's steps, each command with "Run" next to its copy button. */
export function RunSteps({ steps, ask }: { steps: Step[]; ask: Omit<RunAsk, 'command'> }) {
  const run = useContext(RunContext)
  return <Steps steps={steps} onRun={(command, step) => run({ ...ask, command, step })} />
}
