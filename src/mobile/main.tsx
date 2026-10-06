import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import MobileApp from './mobile-app'
import './mobile.css'

const root = document.getElementById('mobile-root')

if (!root) throw new Error('Elemento #mobile-root ausente.')

createRoot(root).render(
  <StrictMode>
    <MobileApp />
  </StrictMode>,
)
