/// <reference types="vite/client" />
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import './styles.css'
import './assets/pokeidle-theme/desktop.css'
import './assets/pokeidle-theme/routes.css'

const startupDebug = import.meta.env.DEV && import.meta.env.MODE !== 'test'
if (startupDebug) {
  console.info('[startup] frontend module graph ready', performance.now().toFixed(1))
  try {
    new PerformanceObserver((entries) => {
      for (const entry of entries.getEntries())
        console.info(`[startup] ${entry.name}`, entry.startTime.toFixed(1))
    }).observe({ type: 'paint', buffered: true })
  } catch {
    // Paint timing may be unavailable in older WebView2 versions.
  }
}
document.addEventListener('DOMContentLoaded', () => {
  if (startupDebug) console.info('[startup] DOMContentLoaded', performance.now().toFixed(1))
}, { once: true })

if (startupDebug) console.info('[startup] React root.render called', performance.now().toFixed(1))
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
