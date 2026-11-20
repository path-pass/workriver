// A button that copies a text (a command, a link).
import { useState } from 'react'
import { Button } from '@/ui/primitives/button'
import { useT } from '@/i18n'
import { cn } from '@/lib/utils'
import { Tip } from './Tip'
import { Icon } from './Icon'

/**
 * Copies `text`; with a `label` it is a small text button, without, an icon (for a command,
 * or what `aria` names, which its tooltip says too).
 */
export function CopyButton({ text, label, aria, className }: { text: string; label?: string; aria?: string; className?: string }) {
  const { t } = useT()
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch { /* no clipboard here: the command can still be selected by hand */ }
  }
  if (label)
    return (
      <Button variant="ghost" size="xs" className="rounded-sm" onClick={copy}>
        <Icon name={copied ? 'check' : 'copy'} className="size-3" />{copied ? t('steps.copied') : label}
      </Button>
    )
  const icon = (
    <Button variant="ghost" size="icon-xs" className={cn('flex-none', className)} aria-label={copied ? t('steps.copied') : aria ?? t('steps.copy')} onClick={copy}>
      <Icon name={copied ? 'check' : aria ? 'permalink' : 'copy'} className="size-3.5" />
    </Button>
  )
  if (!aria) return icon
  // A changed name is not read out: the confirmation is said once, politely.
  return <><Tip text={copied ? t('steps.copied') : aria}>{icon}</Tip><span role="status" className="sr-only">{copied ? t('steps.copied') : ''}</span></>
}
