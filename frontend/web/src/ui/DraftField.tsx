import { memo, useLayoutEffect, useMemo, useRef, type ChangeEventHandler, type ClipboardEventHandler, type ComponentProps, type KeyboardEventHandler } from 'react'
import { Input } from './primitives/input'

type EditHandlers<E> = {
  onChange?: ChangeEventHandler<E>
  onPaste?: ClipboardEventHandler<E>
  onKeyDown?: KeyboardEventHandler<E>
}

/** Keep the field's handlers stable while using the form's latest committed actions. */
function useEditHandlers<E>(handlers: EditHandlers<E>) {
  const current = useRef(handlers)
  useLayoutEffect(() => { current.current = handlers }, [handlers])
  return useMemo<EditHandlers<E>>(() => ({
    onChange: event => current.current.onChange?.(event),
    onPaste: event => current.current.onPaste?.(event),
    onKeyDown: event => current.current.onKeyDown?.(event),
  }), [])
}

const TextField = memo(function TextField(props: ComponentProps<'textarea'>) {
  return <textarea {...props} />
})
const SingleLineField = memo(Input)

export function DraftTextarea({ onChange, onPaste, onKeyDown, ...props }: ComponentProps<'textarea'>) {
  const handlers = useEditHandlers({ onChange, onPaste, onKeyDown })
  return <TextField {...props} {...handlers} />
}

export function DraftInput({ onChange, onPaste, onKeyDown, ...props }: ComponentProps<'input'>) {
  const handlers = useEditHandlers({ onChange, onPaste, onKeyDown })
  return <SingleLineField {...props} {...handlers} />
}
