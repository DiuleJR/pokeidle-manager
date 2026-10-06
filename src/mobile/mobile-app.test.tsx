import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { MobileSnapshot } from './contract'
import { mobileMockSnapshot } from './mock-snapshot'
import MobileApp from './mobile-app'
import { clearMobileInventoryCacheForTests } from './inventory-cache'

let root: Root | undefined
let host: HTMLDivElement | undefined
let source: FakeEventSource | undefined

class FakeEventSource {
  static instances: FakeEventSource[] = []
  onopen: (() => void) | null = null
  onerror: (() => void) | null = null
  closed = false
  listeners = new Map<string, (event: Event) => void>()
  constructor(readonly url: string) {
    FakeEventSource.instances.push(this)
  }
  addEventListener(type: string, listener: (event: Event) => void) {
    this.listeners.set(type, listener)
  }
  close() {
    this.closed = true
  }
  emit(type: string, data?: unknown) {
    this.listeners.get(type)?.(new MessageEvent(type, { data: JSON.stringify(data) }))
  }
}

function renderMobile(query = '') {
  window.history.replaceState({}, '', '/mobile.html' + query)
  host = document.createElement('div')
  document.body.append(host)
  act(() => {
    root = createRoot(host!)
    root.render(<MobileApp />)
  })
  return host
}

async function flush() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0))
    await Promise.resolve()
    await Promise.resolve()
  })
}

function snapshot(revision: number): MobileSnapshot {
  return { ...mobileMockSnapshot, revision, accounts: mobileMockSnapshot.accounts.slice(0, 1) }
}

afterEach(() => {
  act(() => root?.unmount())
  host?.remove()
  root = undefined
  host = undefined
  source = undefined
  FakeEventSource.instances = []
  window.history.replaceState({}, '', '/mobile.html')
  vi.useRealTimers()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
  clearMobileInventoryCacheForTests()
})

describe('mobile data bridge UI', () => {
  it('loads a real baseline, applies only newer SSE revisions, and retains data when disconnected', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request) =>
      String(input).includes('/item-assets')
        ? { ok: true, json: async () => ({ items: [] }) }
        : { ok: true, json: async () => snapshot(10) },
    )
    vi.stubGlobal('fetch', fetchMock)
    vi.stubGlobal('EventSource', FakeEventSource)
    const view = renderMobile()
    await flush()
    source = FakeEventSource.instances[0]

    expect(fetchMock).toHaveBeenCalledWith('/api/v1/mobile/snapshot', { cache: 'no-store' })
    expect(view.textContent).toContain('DemoTrainerThree')
    expect(view.textContent).not.toContain('Dados de demonstração')
    expect(view.querySelectorAll('.mobile-account-card')).toHaveLength(1)
    act(() => source!.emit('snapshot', snapshot(9)))
    expect(view.querySelector('.mobile-account-card')?.textContent).toContain('DemoTrainerThree')

    act(() => source!.emit('snapshot', snapshot(11)))
    expect(view.textContent).toContain('Conectado ao Manager')
    act(() => source!.onerror?.())
    expect(view.textContent).toContain('Desconectado · dados mantidos')
    expect(view.textContent).toContain('DemoTrainerThree')
    expect(source?.closed).toBe(false)
  })

  it('uses mock data only when explicitly requested and visibly labels it', () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    vi.stubGlobal('EventSource', FakeEventSource)
    const view = renderMobile('?mode=mock')

    expect(view.querySelectorAll('.mobile-account-card')).toHaveLength(4)
    expect(view.textContent).toContain('Dados de demonstração')
    expect(view.textContent).toContain('MOCK')
    expect(fetchMock).not.toHaveBeenCalled()
    expect(FakeEventSource.instances).toHaveLength(0)
  })

  it('uses the CSS brand with original local SVG navigation icons', () => {
    const view = renderMobile('?mode=mock')

    expect(view.querySelector('.mobile-brand')?.textContent).toContain('POKEIDLE')
    expect(view.querySelector('.mobile-brand img')).toBeNull()
    expect(view.querySelector('.mobile-page-scenery')?.getAttribute('aria-hidden')).toBe('true')
    expect(view.querySelector('.mobile-page-titleboard > img')).toBeNull()
    expect(view.querySelector('.mobile-page-titleboard h1')?.textContent).toBe('Dashboard')
    expect(view.querySelectorAll('.mobile-nav-icon img[src^="data:image/svg+xml"]')).toHaveLength(5)
    expect(view.querySelectorAll('.mobile-nav-item')).toHaveLength(5)
    expect(
      [...view.querySelectorAll('.mobile-nav-item')].find((item) =>
        item.textContent?.includes('Dashboard'),
      ),
    ).toBeDefined()
  })

  it('loads active Pokémon and currently selected item sprites on the live dashboard', async () => {
    const current = snapshot(17)
    const original = current.accounts[0]!
    current.accounts[0] = {
      ...original,
      activePokemon: {
        name: 'Venusaur',
        level: 90,
        hp: 90,
        maxHp: 100,
        shiny: true,
        looktype: 9101,
        lookShiny: 9102,
      },
      potion: { id: 8101, name: 'Equipped Potion', quantity: 12 },
      ball: { id: 8102, name: 'Selected Ball', quantity: 34 },
    }
    const sprite = {
      assetPath: 'assets/asset-packs/outfits/dashboard-test.png',
      x: 4,
      y: 8,
      width: 16,
      height: 16,
      pageWidth: 64,
      pageHeight: 64,
    }
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      const url = new URL(String(input), 'http://localhost')
      if (url.pathname.endsWith('/pokemon-sprite')) return { ok: true, json: async () => sprite }
      if (url.pathname.endsWith('/item-assets')) {
        const ids = (url.searchParams.get('ids') ?? '').split(',').filter(Boolean)
        return {
          ok: true,
          json: async () => ({
            items: ids.map((id) => ({
              id: Number(id),
              name: id === '8101' ? 'Equipped Potion' : 'Selected Ball',
              assetPath: `assets/site/assets/items/equipped-${id}.png`,
            })),
          }),
        }
      }
      return { ok: true, json: async () => current }
    })
    vi.stubGlobal('fetch', fetchMock)
    vi.stubGlobal('EventSource', FakeEventSource)
    const view = renderMobile()
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0))
    })

    expect(
      fetchMock.mock.calls.some(([url]) =>
        String(url).includes('/pokemon-sprite?looktypes=9102%2C9101'),
      ),
    ).toBe(true)
    expect(view.querySelector('.mobile-pokemon-art .mobile-sprite-frame')).not.toBeNull()
    expect(view.querySelectorAll('.mobile-equipped-item-sprite')).toHaveLength(2)
    expect(view.textContent).toContain('Equipped Potion')
    expect(view.textContent).toContain('Selected Ball')
    expect(view.querySelector('.mobile-summary .glyph-xp')).not.toBeNull()
  })

  it('loads item and Pokémon sprites throughout the live market views', async () => {
    const current = snapshot(18)
    current.market = {
      ...current.market,
      summaries: [],
      topItemSales: [
        {
          currencyGroup: 'all',
          itemName: 'Market Top Item',
          quantity: 12,
          transactions: 4,
          averageUnitPrice: null,
          averageGoldUnitPrice: 50,
          averageOrbUnitPrice: null,
        },
      ],
      recentTransactions: [
        {
          id: 'item-sale',
          occurredAt: 1_790_000_000_000,
          kind: 'item',
          currency: 'gold',
          name: 'Market History Item',
          looktype: null,
          lookShiny: null,
          shiny: false,
          total: 100,
          quantity: 1,
          unitPrice: 100,
        },
        {
          id: 'pokemon-sale',
          occurredAt: 1_790_000_000_000,
          kind: 'pokemon',
          currency: 'orb',
          name: 'Shiny Market Pokémon',
          looktype: 9201,
          lookShiny: 9202,
          shiny: true,
          total: 12,
          quantity: 1,
          unitPrice: 12,
        },
      ],
    }
    const sprite = {
      assetPath: 'assets/asset-packs/outfits/market-test.png',
      x: 4,
      y: 8,
      width: 16,
      height: 16,
      pageWidth: 64,
      pageHeight: 64,
    }
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      const url = new URL(String(input), 'http://localhost')
      if (url.pathname.endsWith('/market/summaries')) {
        return {
          ok: true,
          json: async () => ({
            offset: 0,
            limit: 50,
            total: 1,
            categories: ['stones'],
            items: [
              {
                itemId: 9301,
                name: 'Market Offer Item',
                category: 'stones',
                listings: 2,
                units: 2,
                minGold: 100,
                minOrb: null,
              },
            ],
          }),
        }
      }
      if (url.pathname.endsWith('/item-assets')) {
        const ids = (url.searchParams.get('ids') ?? '').split(',').filter(Boolean)
        const names = url.searchParams.getAll('name')
        return {
          ok: true,
          json: async () => ({
            items: [
              ...ids.map((id) => ({
                id: Number(id),
                name: 'Market Offer Item',
                assetPath: `assets/site/assets/items/market-${id}.png`,
              })),
              ...names.map((name) => ({
                id: null,
                name,
                assetPath: `assets/site/assets/items/${name.toLowerCase().replaceAll(' ', '-')}.png`,
              })),
            ],
          }),
        }
      }
      if (url.pathname.endsWith('/pokemon-sprite')) return { ok: true, json: async () => sprite }
      return { ok: true, json: async () => current }
    })
    vi.stubGlobal('fetch', fetchMock)
    vi.stubGlobal('EventSource', FakeEventSource)
    const view = renderMobile()
    await flush()
    const marketButton = [...view.querySelectorAll<HTMLButtonElement>('.mobile-nav-item')].find(
      (candidate) => candidate.textContent?.includes('Mercado'),
    )!
    act(() => marketButton.click())
    await flush()

    expect(
      view.querySelector('.mobile-offer-row .market-item-artwork img')?.getAttribute('src'),
    ).toContain('market-9301.png')
    const topTab = [...view.querySelectorAll<HTMLButtonElement>('.market-tabs button')].find(
      (candidate) => candidate.textContent === 'Mais vendidos',
    )!
    act(() => topTab.click())
    await flush()
    expect(
      view.querySelector('.mobile-top-sale .market-item-artwork img')?.getAttribute('src'),
    ).toContain('market-top-item.png')

    const recentTab = [...view.querySelectorAll<HTMLButtonElement>('.market-tabs button')].find(
      (candidate) => candidate.textContent === 'Últimas vendas',
    )!
    act(() => recentTab.click())
    await flush()
    expect(
      view.querySelector('.mobile-recent-sale .market-kind-icon img')?.getAttribute('src'),
    ).toContain('market-history-item.png')
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0))
    })
    expect(
      fetchMock.mock.calls.some(([url]) =>
        String(url).includes('/pokemon-sprite?looktypes=9202%2C9201'),
      ),
    ).toBe(true)
    expect(view.querySelector('.mobile-recent-sale .mobile-sprite-frame')).not.toBeNull()
  })

  it('refreshes the real baseline when the app becomes visible again', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue({ ok: true, json: async () => snapshot(fetchMock.mock.calls.length) })
    vi.stubGlobal('fetch', fetchMock)
    vi.stubGlobal('EventSource', FakeEventSource)
    renderMobile()
    await flush()
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' })
    await act(async () => {
      document.dispatchEvent(new Event('visibilitychange'))
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('keeps the hunt timer on one shared visible-only clock', () => {
    vi.useFakeTimers()
    vi.stubGlobal('EventSource', FakeEventSource)
    vi.stubGlobal('fetch', vi.fn())
    const setIntervalSpy = vi.spyOn(window, 'setInterval')
    const clearIntervalSpy = vi.spyOn(window, 'clearInterval')
    const view = renderMobile('?mode=mock')

    expect(view.querySelectorAll('.hunt-timer')).toHaveLength(4)
    expect(view.textContent).toContain('01:02:06')
    expect(setIntervalSpy).toHaveBeenCalledTimes(1)
    expect(setIntervalSpy.mock.calls[0]?.[1]).toBe(1_000)

    act(() => vi.advanceTimersByTime(34_000))
    expect(view.textContent).toContain('01:02:40')
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'hidden' })
    act(() => document.dispatchEvent(new Event('visibilitychange')))
    expect(clearIntervalSpy).toHaveBeenCalledTimes(1)
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' })
    act(() => document.dispatchEvent(new Event('visibilitychange')))
    expect(setIntervalSpy).toHaveBeenCalledTimes(2)
  })

  it('navigates to read-only automation, market, and settings screens', () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    vi.stubGlobal('EventSource', FakeEventSource)
    const view = renderMobile('?mode=mock')

    const clickNavigation = (label: string) => {
      const button = [...view.querySelectorAll<HTMLButtonElement>('.mobile-nav-item')].find(
        (candidate) => candidate.textContent?.includes(label),
      )
      expect(button).toBeDefined()
      act(() => button!.click())
    }

    clickNavigation('Automações')
    expect(view.textContent).toContain('Poções automáticas')
    expect(view.textContent).toContain('Visualização segura')
    expect(view.querySelectorAll('.mobile-automation-card')).toHaveLength(4)

    clickNavigation('Mercado')
    expect(view.textContent).toContain('Aguardando leitura')
    expect(view.textContent).toContain('Beast Ball')
    expect(view.textContent).toContain('O mobile não compra')

    clickNavigation('Mais')
    expect(view.textContent).toContain('Endereço deste dispositivo')
    expect(view.textContent).toContain('Área protegida')
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('loads inventory only on demand and sends bounded read-only page queries', async () => {
    const inventory = {
      accountId: 'mock-trainer-three',
      kind: 'items',
      offset: 0,
      limit: 20,
      total: 1,
      items: [
        { id: '70070', name: 'Golden Potion', category: 'potion', quantity: 105, assetPath: null },
      ],
      pokemon: [],
      types: [],
    }
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      return url.includes('/inventory?')
        ? { ok: true, json: async () => inventory }
        : { ok: true, json: async () => snapshot(10) }
    })
    vi.stubGlobal('fetch', fetchMock)
    vi.stubGlobal('EventSource', FakeEventSource)
    const view = renderMobile()
    await flush()

    expect(fetchMock.mock.calls.some(([url]) => String(url).includes('/inventory?'))).toBe(false)
    const inventoryButton = [...view.querySelectorAll<HTMLButtonElement>('.mobile-nav-item')].find(
      (candidate) => candidate.textContent?.includes('Inventário'),
    )
    act(() => inventoryButton?.click())
    await flush()

    const inventoryCall = fetchMock.mock.calls.find(([url]) => String(url).includes('/inventory?'))
    expect(inventoryCall).toBeDefined()
    expect(String(inventoryCall?.[0])).toContain('kind=items')
    expect(String(inventoryCall?.[0])).toContain('offset=0')
    expect(String(inventoryCall?.[0])).toContain('limit=20')
    expect(view.textContent).toContain('Golden Potion')
    expect(view.querySelector('select[aria-label="Conta do inventário"]')?.textContent).toContain(
      'Todas as contas',
    )
    const requestCount = fetchMock.mock.calls.filter(([url]) =>
      String(url).includes('/inventory?'),
    ).length
    const dashboardButton = [...view.querySelectorAll<HTMLButtonElement>('.mobile-nav-item')].find(
      (candidate) => candidate.textContent?.includes('Dashboard'),
    )
    act(() => dashboardButton?.click())
    act(() => inventoryButton?.click())
    await flush()
    expect(
      fetchMock.mock.calls.filter(([url]) => String(url).includes('/inventory?')),
    ).toHaveLength(requestCount)
  })

  it('loads a bounded inventory page from every account in the all-accounts view', async () => {
    const twoAccounts = {
      ...mobileMockSnapshot,
      revision: 20,
      accounts: mobileMockSnapshot.accounts.slice(0, 2),
    }
    const calls: string[] = []
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      if (url.includes('/inventory?')) {
        calls.push(url)
        const accountId = new URL(url, 'http://localhost').searchParams.get('accountId')!
        const account = twoAccounts.accounts.find((entry) => entry.id === accountId)!
        return {
          ok: true,
          json: async () => ({
            accountId,
            kind: 'items',
            offset: 0,
            limit: 20,
            total: 1,
            items: [
              {
                id: '200',
                name: account.displayName + ' Potion',
                category: 'potion',
                quantity: 3,
                assetPath: null,
              },
            ],
            pokemon: [],
            types: [],
          }),
        }
      }
      return { ok: true, json: async () => twoAccounts }
    })
    vi.stubGlobal('fetch', fetchMock)
    vi.stubGlobal('EventSource', FakeEventSource)
    const view = renderMobile()
    await flush()
    const inventoryButton = [...view.querySelectorAll<HTMLButtonElement>('.mobile-nav-item')].find(
      (candidate) => candidate.textContent?.includes('Inventário'),
    )
    act(() => inventoryButton?.click())
    await flush()

    expect(calls).toHaveLength(2)
    expect(calls.every((url) => url.includes('limit=20'))).toBe(true)
    expect(view.textContent).toContain(twoAccounts.accounts[0].displayName + ' Potion')
    expect(view.textContent).toContain(twoAccounts.accounts[1].displayName + ' Potion')
  })

  it('matches desktop Pokémon filters and renders the cached sprite frame', async () => {
    const current = snapshot(30)
    const inventoryCalls: URL[] = []
    const sprite = {
      assetPath: 'assets/asset-packs/outfits/test.png',
      x: 4,
      y: 8,
      width: 16,
      height: 16,
      pageWidth: 64,
      pageHeight: 64,
    }
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      const url = new URL(String(input), 'http://localhost')
      if (url.pathname.endsWith('/inventory')) {
        inventoryCalls.push(url)
        const kind = url.searchParams.get('kind')
        return {
          ok: true,
          json: async () => ({
            accountId: current.accounts[0].id,
            kind,
            offset: 0,
            limit: 20,
            total: 1,
            items: [],
            pokemon:
              kind === 'pokemon'
                ? [
                    {
                      id: 'p-1',
                      name: 'Ralts',
                      level: 20,
                      shiny: true,
                      speciesId: 280,
                      looktype: 301,
                      lookShiny: 302,
                      quality: 91.5,
                      note: 9.1,
                      power: 5,
                      ivTotal: 120,
                      types: ['psychic', 'fairy'],
                    },
                  ]
                : [],
            types: ['psychic', 'fairy'],
          }),
        }
      }
      if (url.pathname.endsWith('/pokemon-sprite')) return { ok: true, json: async () => sprite }
      return { ok: true, json: async () => current }
    })
    vi.stubGlobal('fetch', fetchMock)
    vi.stubGlobal('EventSource', FakeEventSource)
    const view = renderMobile()
    await flush()
    const inventoryButton = [...view.querySelectorAll<HTMLButtonElement>('.mobile-nav-item')].find(
      (candidate) => candidate.textContent?.includes('Inventário'),
    )
    act(() => inventoryButton?.click())
    await flush()
    const pokemonButton = [
      ...view.querySelectorAll<HTMLButtonElement>('.mobile-segmented button'),
    ].find((candidate) => candidate.textContent === 'Pokémon')
    act(() => pokemonButton?.click())
    await flush()

    const typeSelect = view.querySelector<HTMLSelectElement>(
      'select[aria-label="Tipo de Pokémon"]',
    )!
    act(() => {
      typeSelect.value = 'psychic'
      typeSelect.dispatchEvent(new Event('change', { bubbles: true }))
    })
    const ivInput = view.querySelector<HTMLInputElement>('input[aria-label="IV mínimo"]')!
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(
        ivInput,
        '100',
      )
      ivInput.dispatchEvent(new Event('input', { bubbles: true }))
    })
    const sortSelect = view.querySelector<HTMLSelectElement>(
      'select[aria-label="Ordenar Pokémon"]',
    )!
    act(() => {
      sortSelect.value = 'quality'
      sortSelect.dispatchEvent(new Event('change', { bubbles: true }))
    })
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 275))
    })
    await flush()

    const filteredCall = inventoryCalls.at(-1)!
    expect(filteredCall.searchParams.get('type')).toBe('psychic')
    expect(filteredCall.searchParams.get('minIv')).toBe('100')
    expect(filteredCall.searchParams.get('order')).toBe('quality')
    expect(view.textContent).toContain('Ralts')
    expect(view.textContent).toContain('IV 120')
    expect(
      fetchMock.mock.calls.some(([url]) =>
        String(url).includes('/pokemon-sprite?looktypes=302%2C301'),
      ),
    ).toBe(true)
    expect(
      view.querySelector('.inventory-pokemon-sprite span[style]')?.getAttribute('style'),
    ).toContain('/api/v1/mobile/asset?path=')
  })

  it('pages market offers through the safe summary route and applies currency/category filters server-side', async () => {
    const offerPage = {
      offset: 0,
      limit: 50,
      total: 157,
      categories: ['balls', 'stones'],
      items: [
        {
          itemId: 70000,
          name: 'Beast Ball',
          category: 'balls',
          listings: 95,
          units: 50_976,
          minGold: 170_000,
          minOrb: 2,
        },
      ],
    }
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      return url.includes('/market/summaries?')
        ? { ok: !url.includes('currency=gold'), json: async () => offerPage }
        : { ok: true, json: async () => snapshot(10) }
    })
    vi.stubGlobal('fetch', fetchMock)
    vi.stubGlobal('EventSource', FakeEventSource)
    const view = renderMobile()
    await flush()

    const marketButton = [...view.querySelectorAll<HTMLButtonElement>('.mobile-nav-item')].find(
      (candidate) => candidate.textContent?.includes('Mercado'),
    )
    act(() => marketButton?.click())
    await flush()

    expect(view.textContent).toContain('157 ofertas')
    expect(view.textContent).toContain('Beast Ball')
    const offersButton = [...view.querySelectorAll<HTMLButtonElement>('.mobile-chip')].find(
      (candidate) => candidate.textContent === 'Pokébolas',
    )
    act(() => offersButton?.click())
    await flush()

    const marketCalls = fetchMock.mock.calls
      .map(([url]) => String(url))
      .filter((url) => url.includes('/market/summaries?'))
    expect(marketCalls[0]).toContain('currency=all')
    expect(marketCalls[0]).toContain('limit=50')
    expect(marketCalls[1]).toContain('category=balls')
    expect(view.textContent).toContain('O mobile não compra')

    const goldButton = [...view.querySelectorAll<HTMLButtonElement>('.mobile-chip')].find(
      (candidate) => candidate.textContent === 'Gold',
    )
    act(() => goldButton?.click())
    await flush()
    expect(view.textContent).toContain('Não foi possível carregar as ofertas')
    expect(view.textContent).not.toContain('Beast Ball')
  })
})
