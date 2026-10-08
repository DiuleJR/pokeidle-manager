import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { COMMUNITY } from '../config/community'
import { CommunityProjectCard } from './CommunityProjectCard'

const { openUrlMock } = vi.hoisted(() => ({ openUrlMock: vi.fn() }))
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: openUrlMock }))

let root: Root | undefined
let host: HTMLDivElement | undefined
let clipboardDescriptor: PropertyDescriptor | undefined

function renderCard() {
  host = document.createElement('div')
  document.body.append(host)
  act(() => {
    root = createRoot(host!)
    root.render(<CommunityProjectCard />)
  })
  return host
}

describe('CommunityProjectCard', () => {
  beforeEach(() => {
    openUrlMock.mockReset().mockResolvedValue(undefined)
    clipboardDescriptor = Object.getOwnPropertyDescriptor(navigator, 'clipboard')
  })

  afterEach(() => {
    if (root) act(() => root?.unmount())
    root = undefined
    host?.remove()
    host = undefined
    if (clipboardDescriptor) Object.defineProperty(navigator, 'clipboard', clipboardDescriptor)
    else Reflect.deleteProperty(navigator, 'clipboard')
  })

  it('opens the fixed GitHub and Discord destinations externally', async () => {
    const card = renderCard()
    const github = card.querySelector<HTMLButtonElement>(
      '[aria-label="Abrir o código-fonte do Pokeidle Manager no GitHub"]',
    )!
    const discord = card.querySelector<HTMLButtonElement>(
      '[aria-label="Entrar na comunidade Pokeidle Manager no Discord"]',
    )!

    expect(github.querySelector('.community-link-icon-github path')?.getAttribute('d')).toContain(
      'M12 .297',
    )
    expect(discord.querySelector('.community-link-icon-discord path')?.getAttribute('d')).toContain(
      'M20.317',
    )

    await act(async () => {
      github.click()
      discord.click()
    })

    expect(COMMUNITY.githubUrl).toBe('https://github.com/DiuleJR/pokeidle-manager')
    expect(COMMUNITY.discordUrl).toBe('https://discord.gg/Hup5P6jD7')
    expect(openUrlMock).toHaveBeenNthCalledWith(1, COMMUNITY.githubUrl)
    expect(openUrlMock).toHaveBeenNthCalledWith(2, COMMUNITY.discordUrl)
  })

  it('renders a local SVG QR and copies its exact source-of-truth payload', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    })
    const card = renderCard()
    const qr = card.querySelector<SVGSVGElement>(
      'svg[role="img"][aria-label="QR Code para apoio voluntário via PIX"]',
    )
    const copy = [...card.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent === 'Copiar PIX',
    )!

    expect(qr).not.toBeNull()
    expect(qr?.querySelector('path')?.getAttribute('d')).toBeTruthy()
    expect(card.querySelector('img, iframe')).toBeNull()
    expect(COMMUNITY.pixPayload).toBe(
      '00020126580014br.gov.bcb.pix0136f5af10e3-c705-446d-b0bf-135ebd0e5dc65204000053039865802BR5912Diule Junior6009Sao Paulo62230519daqr7556502979595456304C85E',
    )
    await act(async () => copy.click())

    expect(writeText).toHaveBeenCalledOnce()
    expect(writeText).toHaveBeenCalledWith(COMMUNITY.pixPayload)
    expect(copy.textContent).toBe('PIX copiado!')
    expect(card.textContent).toContain('não libera funções')
  })

  it('shows a discreet error when clipboard access fails', async () => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: vi.fn().mockRejectedValue(new Error('clipboard unavailable')) },
    })
    const card = renderCard()
    const copy = [...card.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent === 'Copiar PIX',
    )!

    await act(async () => copy.click())

    expect(card.textContent).toContain('Não foi possível copiar o PIX.')
    expect(card.textContent).not.toContain(COMMUNITY.pixPayload)
  })
})
