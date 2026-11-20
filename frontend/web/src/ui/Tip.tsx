// A tooltip on any control.
import { type ReactElement, type ReactNode } from 'react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/ui/primitives/tooltip'

export function Tip({ text, children }: { text: ReactNode; children: ReactElement }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent className="wrap-anywhere">{text}</TooltipContent>
    </Tooltip>
  )
}
