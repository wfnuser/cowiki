import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { startLocale } from './lib/i18n.ts'
import { startTheme } from './lib/theme.ts'

// Before the first paint, so dark mode and language apply immediately.
startTheme()
startLocale()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
