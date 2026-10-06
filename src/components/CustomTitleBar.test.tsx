import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'

const windowActions = vi.hoisted(() => ({
  close: vi.fn().mockResolvedValue(undefined),
  minimize: vi.fn().mockResolvedValue(undefined),
  toggleMaximize: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => windowActions,
}))

import { CustomTitleBar } from './CustomTitleBar'

let root: Root | undefined
let host: HTMLDivElement | undefined

function render() {
  host = document.createElement('div')
  document.body.append(host)
  act(() => {
    root = createRoot(host!)
    root.render(<CustomTitleBar />)
  })
  return host
}

afterEach(() => {
  act(() => root?.unmount())
  host?.remove()
  root = undefined
  host = undefined
  windowActions.close.mockClear()
  windowActions.minimize.mockClear()
  windowActions.toggleMaximize.mockClear()
})

describe('CustomTitleBar', () => {
  it('keeps a dedicated native drag region outside the window controls', () => {
    const host = render()
    const dragRegion = host.querySelector('[data-tauri-drag-region]')
    const controls = host.querySelector('.titlebar-controls')

    expect(dragRegion).not.toBeNull()
    expect(dragRegion?.contains(controls)).toBe(false)
    expect(host.textContent).not.toContain('Pokeidle Manager')
  })

  it('keeps maximize disabled while minimize and close remain available', () => {
    const host = render()
    const minimize = host.querySelector<HTMLButtonElement>('button[aria-label="Minimizar"]')!
    const maximize = host.querySelector<HTMLButtonElement>(
      'button[aria-label="Maximizar ou restaurar"]',
    )!
    const close = host.querySelector<HTMLButtonElement>('button[aria-label="Fechar"]')!

    act(() => minimize.click())
    act(() => maximize.click())
    act(() => close.click())

    expect(windowActions.minimize).toHaveBeenCalledTimes(1)
    expect(windowActions.toggleMaximize).not.toHaveBeenCalled()
    expect(windowActions.close).toHaveBeenCalledTimes(1)
    expect(maximize.disabled).toBe(true)
    expect(host.querySelectorAll('.titlebar-controls img[src^="data:image/svg+xml"]')).toHaveLength(3)
  })
})
