import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { UiIcon } from './UiIcon'

export function CustomTitleBar() {
  const [focused, setFocused] = useState(() => document.hasFocus())

  useEffect(() => {
    const onFocus = () => setFocused(true)
    const onBlur = () => setFocused(false)
    window.addEventListener('focus', onFocus)
    window.addEventListener('blur', onBlur)
    return () => {
      window.removeEventListener('focus', onFocus)
      window.removeEventListener('blur', onBlur)
    }
  }, [])

  return (
    <div className={`custom-titlebar${focused ? ' is-focused' : ''}`}>
      <div
        className="titlebar-drag-region"
        data-tauri-drag-region=""
        aria-hidden="true"
        title="Arraste para mover a janela"
      />
      <div className="titlebar-controls" role="group" aria-label="Controles da janela">
        <button
          aria-label="Minimizar"
          title="Minimizar"
          type="button"
          onClick={() => void getCurrentWindow().minimize()}
        >
          <UiIcon name="minimize" />
        </button>
        <button
          aria-label="Maximizar ou restaurar"
          title="Maximizar ou restaurar"
          type="button"
          disabled
        >
          <UiIcon name="maximize" />
        </button>
        <button
          aria-label="Fechar"
          title="Fechar"
          type="button"
          onClick={() => void getCurrentWindow().close()}
        >
          <UiIcon name="close" />
        </button>
      </div>
    </div>
  )
}
