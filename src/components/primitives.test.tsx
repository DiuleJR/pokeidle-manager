import { act, useState } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it } from 'vitest'
import { Card, Switch } from './primitives'

let root: Root | undefined
let host: HTMLDivElement | undefined

function renderSwitch() {
  function Harness() {
    const [checked, setChecked] = useState(false)
    return <Switch label="Switch de teste" checked={checked} onCheckedChange={setChecked} />
  }

  host = document.createElement('div')
  document.body.append(host)
  act(() => {
    root = createRoot(host!)
    root.render(<Harness />)
  })
  return host
}

afterEach(() => {
  act(() => root?.unmount())
  host?.remove()
  root = undefined
  host = undefined
})

describe('Switch', () => {
  it('uses one visible button switch rather than a hidden checkbox', () => {
    const host = renderSwitch()
    const control = host.querySelector<HTMLButtonElement>('button[role="switch"]')

    expect(control).not.toBeNull()
    expect(control?.getAttribute('aria-label')).toBe('Switch de teste')
    expect(control?.getAttribute('aria-checked')).toBe('false')
    expect(host.querySelector('input[type="checkbox"]')).toBeNull()
  })

  it('tracks aria-checked and changes exactly once for click, Space, and Enter', () => {
    const host = renderSwitch()
    const control = host.querySelector<HTMLButtonElement>('button[role="switch"]')!

    act(() => control.click())
    expect(control.getAttribute('aria-checked')).toBe('true')

    act(() => control.dispatchEvent(new KeyboardEvent('keydown', { key: ' ', bubbles: true })))
    expect(control.getAttribute('aria-checked')).toBe('false')

    act(() => control.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })))
    expect(control.getAttribute('aria-checked')).toBe('true')
  })
})

describe('Card', () => {
  it('forwards status attributes used by account-card visual states', () => {
    host = document.createElement('div')
    document.body.append(host)
    act(() => {
      root = createRoot(host!)
      root.render(<Card data-status="online">Conta</Card>)
    })

    expect(host.querySelector('.card')?.getAttribute('data-status')).toBe('online')
  })
})
