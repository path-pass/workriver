import { createContext } from 'react'

/** The page's action for following a workriver address in place. */
export const NavigateContext = createContext<((path: string) => void) | null>(null)
