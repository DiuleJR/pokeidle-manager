import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'

function MinimizeIcon() {
  return <svg aria-hidden="true" viewBox="0 0 20 20"><path d="M5 10.5h10" /></svg>
}

function MaximizeIcon() {
  return <svg aria-hidden="true" viewBox="0 0 20 20"><rect x="5.5" y="5.5" width="9" height="9" rx="0.5" /></svg>
}

function CloseIcon() {
  return <svg aria-hidden="true" viewBox="0 0 20 20"><path d="m6 6 8 8M14 6l-8 8" /></svg>
}

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
          <MinimizeIcon />
        </button>
        <button
          aria-label="Maximizar ou restaurar"
          title="Maximizar ou restaurar"
          type="button"
          disabled
        >
          <MaximizeIcon />
        </button>
        <button
          aria-label="Fechar"
          title="Fechar"
          type="button"
          onClick={() => void getCurrentWindow().close()}
        >
          <CloseIcon />
        </button>
      </div>
    </div>
  )
}
