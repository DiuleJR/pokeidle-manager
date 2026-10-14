import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { emptyLiveSnapshot, type LiveSnapshot } from './types'
import { LivesPage } from './LivesPage'

let root: Root | undefined
let host: HTMLDivElement | undefined

function renderPage(snapshot: LiveSnapshot, onOpenLive = vi.fn().mockResolvedValue(undefined)) {
  host = document.createElement('div')
  document.body.append(host)
  act(() => {
    root = createRoot(host!)
    root.render(<LivesPage snapshot={snapshot} onOpenLive={onOpenLive} />)
  })
  return { page: host, onOpenLive }
}

function withSource(overrides: Partial<LiveSnapshot> = {}): LiveSnapshot {
  return {
    ...emptyLiveSnapshot,
    sourceAccountId: 'account-1',
    sourceAccountNick: 'ContaTeste',
    sourceTransport: 'background',
    ...overrides,
  }
}

afterEach(() => {
  if (root) act(() => root?.unmount())
  root = undefined
  host?.remove()
  host = undefined
})

describe('LivesPage', () => {
  it('renders Twitch and online Kick channels, and opens each fixed channel identifier', async () => {
    const { page, onOpenLive } = renderPage(
      withSource({
        twitch: [{ login: 'PokeIdle', name: 'Poke Idle', viewers: 1234 }],
        kick: [
          { slug: 'official', name: 'Official Stream', viewers: 456, online: true, active: true },
          { slug: 'offline', name: 'Offline Stream', viewers: 8000, online: false, active: true },
        ],
      }),
    )

    expect(page.textContent).toContain('Poke Idle')
    expect(page.textContent).toContain('Official Stream')
    expect(page.textContent).not.toContain('Offline Stream')
    const twitchMarks = page.querySelectorAll<HTMLImageElement>(
      '.lives-platform-mark-twitch img',
    )
    expect(twitchMarks.length).toBeGreaterThan(0)
    expect([...twitchMarks].every((image) => image.src.includes('icon-twitch.png'))).toBe(true)

    const twitchButton = page.querySelector<HTMLButtonElement>(
      'button[aria-label="Abrir Poke Idle na Twitch"]',
    )!
    const kickButton = page.querySelector<HTMLButtonElement>(
      'button[aria-label="Abrir Official Stream na Kick"]',
    )!
    await act(async () => {
      twitchButton.click()
      kickButton.click()
    })

    expect(onOpenLive).toHaveBeenNthCalledWith(1, 'twitch', 'PokeIdle')
    expect(onOpenLive).toHaveBeenNthCalledWith(2, 'kick', 'official')
  })

  it('shows the no-source empty state when no reader is selected', () => {
    const { page } = renderPage(emptyLiveSnapshot)
    expect(page.textContent).toContain('Aguardando uma conta leitora')
    expect(page.textContent).toContain('Nenhuma conta está lendo as lives.')
    expect(page.textContent).toContain('Conecte uma conta para carregar as lives.')
  })

  it('distinguishes a connected source that has not sent its first lives snapshot', () => {
    const { page } = renderPage(withSource())
    expect(page.textContent).toContain('Aguardando os primeiros dados de lives.')
    expect(page.textContent).toContain('Aguardando a lista da Twitch.')
    expect(page.textContent).toContain('Aguardando a lista da Kick.')
    expect(page.textContent).not.toContain('Nenhuma live da Twitch está online')
  })

  it('shows stale state while retaining the last known channel data', () => {
    const { page } = renderPage(
      withSource({
        stale: true,
        twitch: [{ login: 'stillhere', name: 'Still Here', viewers: 3 }],
        updatedAt: Date.parse('2026-10-13T10:00:00Z'),
      }),
    )
    expect(page.textContent).toContain('Dados desatualizados')
    expect(page.textContent).toContain('Os últimos dados recebidos continuam visíveis.')
    expect(page.textContent).toContain('Still Here')
  })

  it('sorts viewers descending and names ascending, keeping the last duplicate identifier', () => {
    const { page } = renderPage(
      withSource({
        twitch: [
          { login: 'same', name: 'Old duplicate', viewers: 99 },
          { login: 'zulu', name: 'Zulu', viewers: 10 },
          { login: 'ALPHA', name: 'Alpha', viewers: 10 },
          { login: 'SAME', name: 'Latest duplicate', viewers: 20 },
        ],
      }),
    )
    const cards = [
      ...page.querySelectorAll(
        '.lives-platform-section[aria-labelledby="lives-twitch-title"] .lives-stream-card',
      ),
    ]
    const names = cards.map((card) => card.querySelector('h3')?.textContent)
    expect(names).toEqual(['Latest duplicate', 'Alpha', 'Zulu'])
  })

  it('marks unsafe or malformed channel identifiers unavailable instead of creating a link', () => {
    const { page, onOpenLive } = renderPage(
      withSource({
        twitch: [{ login: 'bad/login', name: 'Bad Twitch', viewers: 2 }],
        kick: [{ slug: 'bad/slug', name: 'Bad Kick', viewers: 1, online: true, active: true }],
      }),
    )
    expect(page.textContent).toContain('Bad Twitch')
    expect(page.textContent).toContain('Bad Kick')
    expect(page.textContent.match(/Link indisponível/g)).toHaveLength(2)
    expect(page.querySelectorAll('.lives-open-button')).toHaveLength(0)
    expect(onOpenLive).not.toHaveBeenCalled()
  })

  it('shows a discreet message if opening a valid channel fails', async () => {
    const onOpenLive = vi.fn().mockRejectedValue(new Error('shell error'))
    const { page } = renderPage(
      withSource({
        kick: [{ slug: 'channel', name: 'Channel', viewers: 1, online: true, active: true }],
      }),
      onOpenLive,
    )
    const button = page.querySelector<HTMLButtonElement>(
      'button[aria-label="Abrir Channel na Kick"]',
    )!
    await act(async () => button.click())
    expect(page.textContent).toContain('Não foi possível abrir o canal.')
  })
})
