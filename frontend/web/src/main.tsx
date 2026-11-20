import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import App from './App'
import { Session } from '@/session/Session'
import './i18n'
import { applyTheme, readTheme } from './theme'
import './index.css'

applyTheme(readTheme())

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <TooltipProvider delayDuration={400}>
      <Session><App /></Session>
    </TooltipProvider>
  </StrictMode>,
)
