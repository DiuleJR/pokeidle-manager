import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'
import App from './App'
import { emptyCatalog } from './inventory/assets'
import type { HuntReference } from './hunts/calculator'
import { mergeMarketCatalog } from './inventory/market-catalog'
import { mockAccounts } from './mocks/accounts'
import { useAppStore } from './stores/app-store'
import { useMarketStore } from './stores/market-store'
import type { MarketSnapshot } from './types'
import type { IntegrationLiveSnapshot, IntegrationSnapshot } from './real-account'

const { invokeMock, listenMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(),
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }))

let root: Root | undefined
let host: HTMLDivElement | undefined
function render() {
  host = document.createElement('div')
  document.body.append(host)
  act(() => {
    root = createRoot(host!)
    root.render(<App />)
  })
  return host
}

function setupDesktop(
  onCommand: (command: string, payload?: Record<string, unknown>) => unknown = () => null,
  onIntegrationSnapshot: () => unknown = () => ({ accounts: [], diagnostic: { lifecycle: 'closed', state: 'Aguardando', message: '', accountId: null } }),
) {
  Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
  listenMock.mockResolvedValue(() => {})
  invokeMock.mockImplementation((command: string, payload?: Record<string, unknown>) => {
    if (command === 'app_core_status') return Promise.resolve({ ready: true, error: null })
    if (command === 'dashboard' || command === 'dashboard_live') return Promise.resolve([])
    if (command === 'market_snapshot') return Promise.resolve({})
    if (command === 'settings_snapshot') return Promise.resolve({ minimizeToTray: true, gameUrl: 'https://pokeidle.io/app', startupBrowserConcurrency: 1 })
    if (command === 'integration_live_snapshot')
      return Promise.resolve(toLiveSnapshot(onIntegrationSnapshot() as IntegrationSnapshot))
    return onCommand(command, payload)
  })
  act(() => {
    useAppStore.getState().setSetting('mockMode', false)
    useAppStore.setState({ real: { accounts: [] } })
    useAppStore.getState().setPage('Dashboard')
  })
}

function toLiveSnapshot(snapshot: IntegrationSnapshot): IntegrationLiveSnapshot {
  return {
    diagnostic: snapshot.diagnostic,
    accounts: snapshot.accounts.map(({ account, state, metrics }) => ({
      account,
      state: {
        level: state.level,
        xp: state.xp,
        gold: state.gold,
        diamonds: state.diamonds,
        orbs: state.orbs,
        vip_until: state.vip_until,
        vip_active: state.vip_active,
        server_now: state.server_now,
        vip_data_available: state.vip_data_available,
        xp_bonus: state.xp_bonus,
        center_free_at: state.center_free_at,
        combat_lock_until: state.combat_lock_until,
        server_offset_ms: state.server_offset_ms,
        combat_locked: state.combat_locked,
        command_transport_available: state.command_transport_available,
        pending_navigation: state.pending_navigation,
        navigation_error: state.navigation_error,
        automation_error: state.automation_error,
        hunt_slug: state.hunt_slug,
        hunt_started_at_ms: state.hunt_started_at_ms,
        pending_hunt_slug: state.pending_hunt_slug,
        no_centro: state.no_centro,
        activity: state.activity,
        active_potion_source: state.active_potion_source,
        wild: state.wild,
        auto_buy_rules: state.auto_buy_rules,
        capture_mode: state.capture_mode,
        capture_queue_len: state.capture_queue_len,
        capture_error: state.capture_error,
        automation: state.automation,
        hunt_session: state.hunt_session,
        active_pokemon:
          state.active_id === undefined
            ? null
            : (state.pokemon.find((pokemon) => pokemon.id === state.active_id) ?? null),
        active_hunt: null,
      },
      metrics,
      current_potion_id: null,
      current_potion_quantity: 0,
      current_ball_id: null,
      current_ball_quantity: 0,
      revisions: { depot: 1, inventory: 1, hunt_options: 1 },
    })),
  }
}

async function flushDesktopEffects() {
  await act(async () => {
    for (let index = 0; index < 12; index += 1) await Promise.resolve()
  })
}

afterEach(() => {
  act(() => root?.unmount())
  host?.remove()
  root = undefined
  host = undefined
  vi.useRealTimers()
  Reflect.deleteProperty(window, '__TAURI_INTERNALS__')
  invokeMock.mockReset()
  listenMock.mockReset()
  useMarketStore.setState({ snapshot: null })
})

describe('Pokeidle Manager UI', () => {
  it('starts Community without a license handshake and waits for local initialization', () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    listenMock.mockResolvedValue(() => {})
    invokeMock.mockReturnValue(new Promise(() => {}))

    const host = render()
    expect(host.querySelector('.startup-shell')).not.toBeNull()
    expect(host.querySelector('.startup-page')?.textContent).toContain('Preparando dados…')
    expect(host.textContent).not.toContain('licença')
    expect(invokeMock.mock.calls.some(([command]) => String(command).includes('license'))).toBe(false)
  })

  it('shows the application shell while account hydration is still pending', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    listenMock.mockResolvedValue(() => {})
    invokeMock.mockImplementation((command: string) => {
      if (command === 'app_core_status') return Promise.resolve({ ready: true, error: null })
      return new Promise(() => {})
    })

    const host = render()
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.app-shell')).not.toBeNull()
    expect(host.textContent).toContain('Carregando contas persistidas...')
  })

  it('shows a non-interactive shell while the local core prepares', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    listenMock.mockResolvedValue(() => {})
    let resolveCore: ((value: unknown) => void) | undefined
    invokeMock.mockImplementation((command: string) => {
      if (command === 'app_core_status')
        return new Promise((resolve) => {
          resolveCore = resolve
        })
      return new Promise(() => {})
    })

    const host = render()
    expect(host.querySelector('.startup-shell')).not.toBeNull()
    expect(host.querySelector('.startup-page')?.textContent).toContain('Preparando dados…')
    expect([...host.querySelectorAll<HTMLButtonElement>('.startup-shell nav button')].every(
      (button) => button.disabled,
    )).toBe(true)
    expect(invokeMock.mock.calls.some(([command]) => command === 'dashboard_live')).toBe(false)

    await act(async () => {
      resolveCore?.({ ready: true, error: null })
    })
    expect(host.querySelector('.startup-shell')).toBeNull()
    expect(host.querySelector('.app-shell')).not.toBeNull()
    expect(invokeMock.mock.calls.some(([command]) => String(command).includes('license'))).toBe(false)
  })

  it('shows the normal empty dashboard instead of startup diagnostics in a production build', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    listenMock.mockResolvedValue(() => {})
    const diagnostic = {
      state: 'Aguardando',
      message: 'Nenhuma integração em andamento.',
      accountId: null,
    } as IntegrationSnapshot['diagnostic']
    invokeMock.mockImplementation((command: string) => {
      if (command === 'app_core_status')
        return Promise.resolve({ ready: true, error: null })
      if (command === 'dashboard' || command === 'dashboard_live') return Promise.resolve([])
      if (command === 'settings_snapshot')
        return Promise.resolve({
          minimizeToTray: true,
          gameUrl: 'https://pokeidle.io/app',
          startupBrowserConcurrency: 1,
        })
      if (command === 'integration_live_snapshot')
        return Promise.resolve(
          toLiveSnapshot({ accounts: [], diagnostic } as IntegrationSnapshot),
        )
      if (command === 'market_snapshot') return Promise.resolve({})
      return Promise.resolve(null)
    })

    const host = render()
    await act(async () => {
      for (let index = 0; index < 8; index += 1) await Promise.resolve()
    })

    expect(host.querySelector('.empty-state')).not.toBeNull()
    expect(host.querySelector('.integration-panel')).toBeNull()
    expect(host.textContent).not.toContain('INTEGRAÇÃO REAL')
    expect(host.textContent).toContain('Nenhuma conta cadastrada')
  })

  it('describes the free Community build in Settings without activation controls', () => {
    act(() => useAppStore.getState().setPage('Configurações'))
    const host = render()

    expect(host.textContent).toContain('PROJETO OPEN SOURCE')
    expect(host.textContent).toContain('Community Build')
    expect(host.textContent).toContain('Gratuito')
    expect(host.textContent).toContain('GPL-3.0-only')
    expect(host.querySelector('input[type="password"]')).toBeNull()
    expect([...host.querySelectorAll('button')].some((button) => /ativar licença|remover licença/i.test(button.textContent ?? ''))).toBe(false)
  })

  it('keeps a verified catalog sprite when sparse Market metadata omits its icon', () => {
    const catalog = mergeMarketCatalog(
      {
        ...emptyCatalog,
        items: {
          '40': {
            id: 40,
            name: 'Earth Stone',
            category: 'stone',
            assetPath: 'assets/site/assets/items/earth_stone.gif',
          },
        },
      },
      [
        {
          itemId: 40,
          name: 'Earth Stone',
          category: 'stone',
          assetPath: null,
          updatedAt: 1,
        },
      ],
    )

    expect(catalog.items['40']?.assetPath).toBe('assets/site/assets/items/earth_stone.gif')
  })

  it('renders a visible Dashboard hydration state before account runtime data arrives', () => {
    const host = render()
    expect(host.textContent).toContain('Dashboard')
    expect(host.textContent).toContain('Carregando contas persistidas...')
  })
  it.each(['Dashboard', 'Automações', 'Inventários', 'Mercado', 'Configurações'] as const)(
    'uses the shared CSS title plaque on the %s tab',
    (page) => {
      act(() => useAppStore.getState().setPage(page))
      const host = render()
      const header = host.querySelector<HTMLElement>('.shared-page-header')

      expect(header?.querySelector('h1')?.textContent).toBe(page)
      expect(header?.querySelector('.shared-page-header-scenery')).not.toBeNull()
      expect(header?.querySelector('.shared-page-header-plaque > img')).not.toBeNull()
      expect(header?.querySelector('.shared-page-header-copy p')).toBeNull()
    },
  )
  it('shows active XP modifiers and IV stats with local SVG icons', () => {
    act(() => {
      useAppStore.setState((state) => ({
        real: {
          accounts: [{
            ...mockAccounts[0],
            vip: true,
            activePokemon: {
              id: 'active-venusaur',
              name: 'Venusaur',
              level: 700,
              quality: 1.408,
              potency: 2,
              types: ['GRASS', 'POISON'],
              ivs: { hp: 21, atk: 16, def: 22, spAtk: 32, spDef: 26, speed: 32 },
            },
            xpBonus: {
              guildRankPct: 5,
              guildBoostActive: true,
              twitchPct: 22.5,
              eventTrainerPct: 10,
              eventPokemonPct: 10,
            },
          }],
        },
        ui: { ...state.ui, page: 'Onde Caçar' },
      }))
    })

    const host = render()
    const rows = [...host.querySelectorAll<HTMLElement>('.where-hunt-bonus-row')]

    expect(rows.map((row) => row.dataset.bonus)).toEqual(['vip', 'guild', 'guild-boost', 'twitch', 'event'])
    expect(rows.map((row) => row.querySelector('.where-hunt-bonus-value')?.textContent)).toEqual([
      '+50%', '+5%', '+10%', '+22,5%', '+10%',
    ])
    expect(rows.slice(0, 4).every((row) => {
      const icon = row.querySelector('.where-hunt-bonus-icon')
      return icon?.getAttribute('aria-hidden') === 'true' && icon.querySelector('img')
    })).toBe(true)
    expect(host.querySelector('.where-hunt-bonus-title')?.textContent).toContain('BÔNUS CONSIDERADOS')
    const ivCards = [...host.querySelectorAll<HTMLElement>('.where-hunt-iv-card')]
    expect(host.querySelector('.where-hunt-potency-p2 .badge')?.textContent).toBe('P2')
    expect(ivCards.map((card) => card.querySelector('small')?.textContent)).toEqual([
      'HP', 'ATK', 'DEF', 'SP. ATK', 'SP. DEF', 'SPD',
    ])
    expect(ivCards.every((card) => card.querySelector('.where-hunt-iv-icon img'))).toBe(true)
    expect(ivCards.map((card) => card.querySelector('strong')?.textContent)).toEqual([
      '21', '16', '22', '32', '26', '32',
    ])
  })
  it('keeps inactive XP bonus rows visible and marked OFF', () => {
    act(() => {
      useAppStore.setState((state) => ({
        real: { accounts: [{ ...mockAccounts[0], vip: false, xpBonus: undefined }] },
        ui: { ...state.ui, page: 'Onde Caçar' },
      }))
    })

    const host = render()
    const rows = [...host.querySelectorAll<HTMLElement>('.where-hunt-bonus-row')]

    expect(rows.map((row) => row.dataset.bonus)).toEqual(['vip', 'guild', 'guild-boost', 'twitch', 'event'])
    expect(rows.every((row) => row.dataset.active === 'false')).toBe(true)
    expect(rows.every((row) => row.querySelector('.where-hunt-bonus-value')?.textContent === 'OFF')).toBe(true)
  })
  it('keeps the selected account in Onde Caçar when switching tabs', () => {
    act(() =>
      useAppStore.setState((state) => ({
        real: { accounts: [mockAccounts[0], mockAccounts[1]] },
        ui: { ...state.ui, page: 'Onde Caçar' },
      })),
    )

    const host = render()
    const accountSelect = host.querySelector<HTMLSelectElement>('#where-hunt-account')!
    act(() => {
      accountSelect.value = mockAccounts[1].id
      accountSelect.dispatchEvent(new Event('change', { bubbles: true }))
    })
    expect(useAppStore.getState().ui.whereToHuntAccountId).toBe(mockAccounts[1].id)

    act(() => useAppStore.getState().setPage('Dashboard'))
    act(() => useAppStore.getState().setPage('Onde Caçar'))

    expect(host.querySelector<HTMLSelectElement>('#where-hunt-account')?.value).toBe(
      mockAccounts[1].id,
    )
  })
  it('keeps loaded hunt recommendations ready when returning to the tab', () => {
    const reference: HuntReference = {
      tipos: { ROCK: { ROCK: 1 }, BUG: { ROCK: 1 }, GHOST: { ROCK: 1 } },
      especies: [
        {
          id: 1,
          n: 'Tyranitar',
          t: ['ROCK'],
          b: [100, 134, 110, 95, 100, 61],
          m: [['Rock Slide', 'ROCK', 0, 75, 20_000, 1]],
        },
        { id: 10, n: 'Shedinja', t: ['BUG', 'GHOST'], b: [1, 90, 45, 30, 30, 40], m: [] },
      ],
      hunts: [{ s: 'shedinja', n: 'Shedinja', a: 'Kanto', nv: 10, e: [[10, 15]] }],
    }
    const account = {
      ...mockAccounts[0],
      level: 100,
      trainerLevel: 100,
      activePokemon: {
        id: 'active-tyranitar',
        name: 'Tyranitar',
        level: 100,
        speciesId: 1,
        quality: 1,
        potency: 1,
        types: ['ROCK'],
        ivs: { hp: 16, atk: 16, def: 16, spAtk: 16, spDef: 16, speed: 16 },
      },
    }
    act(() =>
      useAppStore.setState((state) => ({
        real: { accounts: [account] },
        whereHuntReference: reference,
        ui: { ...state.ui, page: 'Onde Caçar' },
      })),
    )

    const host = render()
    expect(host.querySelectorAll('.where-hunt-result')).toHaveLength(1)

    act(() => useAppStore.getState().setPage('Dashboard'))
    act(() => useAppStore.getState().setPage('Onde Caçar'))

    expect(host.querySelectorAll('.where-hunt-result')).toHaveLength(1)
    expect(invokeMock.mock.calls.some(([command]) => command === 'where_to_hunt_reference')).toBe(false)
  })
  it('renders the Mercado workspace with the compact market pulse panels', () => {
    act(() => useAppStore.setState({ ui: { ...useAppStore.getState().ui, page: 'Mercado' } }))
    const host = render()

    expect(host.textContent).toContain('Mercado')
    expect(host.textContent).toContain('ANÁLISE DO MERCADO')
    expect(host.textContent).not.toContain('Pulso do Mercado')
    expect(host.textContent).toContain('Mais vendidos')
    expect(host.textContent).toContain('Últimas vendas')
    expect(host.textContent).toContain('Ofertas ativas')
    expect(host.querySelector('.market-pulse-heading .eyebrow')).toBeNull()
    expect(host.querySelector('.market-offers-heading .eyebrow')).toBeNull()
    expect(host.querySelector('.market-rule-editor')).not.toBeNull()
    expect(host.textContent).toContain('COMPRA AUTOMÁTICA')
    expect(host.textContent).toContain('Proteções avançadas')
    expect(host.textContent).toContain('Preço máximo por unidade')
    expect(host.textContent).not.toContain('Quantidade por oferta')
    expect(host.textContent).not.toContain('Adicionar regra')
    expect(host.textContent).toContain('Compras e sorteios')
    expect(host.textContent).toContain('Sorteios perdidos não descontam saldo.')
    expect(host.querySelector('.market-table-scroll')).not.toBeNull()
    expect(host.querySelectorAll('.market-pulse-card')).toHaveLength(2)
    expect(host.querySelector('.market-history-virtualizer')).not.toBeNull()
    act(() => useAppStore.setState({ ui: { ...useAppStore.getState().ui, page: 'Dashboard' } }))
  })
  it('keeps the last market snapshot visible after leaving and returning to the tab', () => {
    const snapshot: MarketSnapshot = {
      version: 10,
      readerAccountId: 'reader-1',
      readerStatus: 'Leitura contínua',
      lastMarketError: null,
      lastUpdatedAt: 10,
      summaries: [],
      itemMetadata: [],
      rules: [],
      purchases: [],
      purchaseHistoryTruncated: false,
      historyStatus: 'Histórico atualizado',
      historyLastUpdatedAt: 10,
      recentTransactions: [],
      topItemSales: [
        {
          itemName: 'Cached Earth Stone',
          quantity: 4,
          transactions: 2,
          currency: null,
          averageUnitPrice: null,
          averageGoldUnitPrice: 100,
          averageOrbUnitPrice: null,
        },
      ],
      transactionHistoryConfirmed: true,
    }
    act(() => useMarketStore.getState().setSnapshot(snapshot))
    act(() => useAppStore.getState().setPage('Mercado'))
    const host = render()

    expect(host.textContent).toContain('Cached Earth Stone')
    act(() => useAppStore.getState().setPage('Dashboard'))
    expect(host.querySelector('.market-page')).toBeNull()
    act(() => useAppStore.getState().setPage('Mercado'))
    expect(host.textContent).toContain('Cached Earth Stone')
    expect(host.textContent).not.toContain('Aguardando a primeira leitura.')
  })
  it('labels an unconfirmed local market outcome as uncertain, not purchased', async () => {
    const snapshot = {
      version: 1,
      readerAccountId: null,
      readerStatus: 'waiting',
      lastMarketError: null,
      lastUpdatedAt: null,
      summaries: [],
      itemMetadata: [],
      rules: [],
      purchases: [
        {
          id: 'uncertain-1',
          purchasedAt: 1_000,
          accountId: 'missing',
          ruleId: 'rule-1',
          listingId: 4,
          itemId: 40,
          description: 'Earth Stone',
          quantity: 2,
          total: 900,
          currency: 'gold' as const,
          outcome: 'uncertain' as const,
        },
        {
          id: 'purchased-1',
          purchasedAt: 2_000,
          accountId: 'missing',
          ruleId: 'rule-2',
          listingId: 5,
          itemId: 40,
          description: 'Earth Stone',
          quantity: 1,
          total: 500,
          currency: 'gold' as const,
          outcome: 'purchased' as const,
        },
      ],
      purchaseHistoryTruncated: false,
      historyStatus: 'ready',
      historyLastUpdatedAt: null,
      recentTransactions: [],
      topItemSales: [],
      transactionHistoryConfirmed: true,
    }
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    listenMock.mockResolvedValue(() => {})
    invokeMock.mockImplementation((command: string) => {
      if (command === 'app_core_status') return Promise.resolve({ ready: true, error: null })
      if (command === 'market_snapshot') return Promise.resolve(snapshot)
      if (command === 'dashboard' || command === 'dashboard_live') return Promise.resolve([])
      if (command === 'settings_snapshot')
        return Promise.resolve({
          minimizeToTray: true,
          gameUrl: 'https://pokeidle.io/app',
          startupBrowserConcurrency: 1,
        })
      if (command === 'integration_live_snapshot')
        return Promise.resolve(toLiveSnapshot({
          accounts: [],
          diagnostic: {
            lifecycle: 'closed',
            sessionMode: null,
            braveFound: false,
            profileCreated: false,
            accountPersisted: false,
            persistentProfile: false,
            braveStarted: false,
            cdpPort: null,
            cdpEndpointAvailable: false,
            cdpEndpointAttempts: 0,
            cdpEndpointLastError: null,
            browserProduct: null,
            browserWsUrlObtained: false,
            browserWsConnected: false,
            cdpConnected: false,
            targetFound: false,
            targetIdFound: false,
            sessionCreated: false,
            managedGamePage: false,
            pageEnabled: false,
            pageNavigateSent: false,
            gameUrlNavigated: false,
            finalUrl: null,
            networkEnabled: false,
            websocketCount: 0,
            pokeidleSocketDetected: false,
            websocketDetected: false,
            helloDetected: false,
            welcomeReceived: false,
            wsUrlCaptured: false,
            sessionMaterialCaptured: false,
            rustWsConnected: false,
            rustHelloSent: false,
            rustWelcomeReceived: false,
            browserClosed: false,
            backgroundActive: false,
            controlledBravePid: null,
            automaticReloadUsed: false,
            nick: null,
            state: 'closed',
            message: '',
            accountId: null,
          },
        } as IntegrationSnapshot))
      return Promise.resolve(undefined)
    })
    act(() => useAppStore.getState().setPage('Mercado'))
    const host = render()
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.market-history-status.uncertain')?.textContent).toBe(
      'Resultado incerto',
    )
    expect(host.querySelector('.market-history-status.purchased')?.textContent).toBe('Comprada')
    expect(host.querySelector('.uncertain-result')?.textContent).toContain('Não confirmado')
  })
  it('shows four available account slots in demonstration mode', () => {
    act(() => useAppStore.getState().setSetting('mockMode', true))
    const host = render()
    expect(host.textContent).toContain('DemoTrainerFive')
    expect(host.textContent).toContain('MODO DEMONSTRAÇÃO')
  })
  it.each([1, 2, 4])('keeps the premium Dashboard grid bounded with %i account(s)', (count) => {
    const accounts = mockAccounts.slice(0, count).map((account, index) => ({
      ...account,
      id: `premium-${index}`,
    }))
    act(() => useAppStore.setState({ real: { accounts } }))
    const host = render()

    expect(host.querySelector('.shared-page-header')).not.toBeNull()
    expect(host.querySelectorAll('.premium-total')).toHaveLength(5)
    expect(host.querySelector('.premium-total-gems .game-gem-icon')).not.toBeNull()
    expect(host.querySelectorAll('.account-grid .premium-account-card')).toHaveLength(count)
    expect(host.querySelector('.account-grid')?.classList.contains('account-grid')).toBe(true)
  })
  it('keeps the Dashboard header direct and account HP compact', () => {
    act(() => useAppStore.setState({ real: { accounts: [mockAccounts[0]] } }))
    const host = render()
    const accountCard = host.querySelector('.premium-account-card')!

    expect(host.querySelector('.shared-page-header-copy .eyebrow')).toBeNull()
    expect(host.querySelector('.brand .sidebar-brand-art')?.getAttribute('alt')).toBe('Pokeidle Manager')
    expect(host.querySelectorAll('nav .sidebar-nav-icon img')).toHaveLength(6)
    expect(host.textContent).not.toContain('Sua farm em uma só visão.')
    expect(accountCard.querySelector('.premium-hp-row')?.textContent).toContain('%')
    expect(accountCard.querySelector('.premium-hp-row small')).toBeNull()
  })
  it('shows the account Gold and Gem balances beside its Hunt', () => {
    act(() => useAppStore.setState({ real: { accounts: [mockAccounts[1]] } }))
    const host = render()
    const footer = host.querySelector('.premium-account-footer')!

    expect(footer.textContent).toContain('Seafoam Route')
    expect(footer.textContent).toContain('16.205.900')
    expect(footer.textContent).toContain('18')
    expect(footer.querySelectorAll('.premium-account-balance')).toHaveLength(2)
    expect(footer.querySelector('.premium-account-balance-gold .game-gold-icon')).not.toBeNull()
    expect(footer.querySelector<HTMLImageElement>('.game-gem-icon')?.src).toContain('pokeidle.io/img/moeda-gema.png')
  })
  it('shows and updates a persistent per-account hunt timer', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-10-01T12:00:05.000Z'))
    act(() =>
      useAppStore.setState({
        real: {
          accounts: [{ ...mockAccounts[1], huntStartedAtMs: Date.parse('2026-10-01T12:00:00.000Z') }],
        },
      }),
    )
    const host = render()

    expect(host.querySelector('.premium-account-hunt-icon')?.tagName).toBe('IMG')
    expect(host.querySelector('.premium-account-hunt-icon')?.getAttribute('aria-hidden')).toBe('true')
    expect(host.querySelector('.premium-account-hunt-timer')?.textContent).toBe('00:05')
    expect(host.querySelector('.premium-account-hunt-timer')?.getAttribute('aria-label')).toBe(
      'Tempo desde a seleção da hunt: 00:05',
    )
    act(() => vi.advanceTimersByTime(2000))
    expect(host.querySelector('.premium-account-hunt-timer')?.textContent).toBe('00:07')
  })
  it('keeps Add account available through four real accounts and exposes the limit at four', () => {
    const accounts = mockAccounts.map((account, index) => ({ ...account, id: `real-${index}` }))
    act(() => useAppStore.setState({ real: { accounts } }))
    const host = render()
    const add = [...host.querySelectorAll('button')].find(
      (button) => button.textContent === '+ Adicionar conta',
    )!
    expect(add.disabled).toBe(true)
    expect(host.textContent).toContain('Limite de 4 contas atingido')
  })
  it('applies an automation only to selected accounts', () => {
    act(() => useAppStore.getState().setSetting('mockMode', true))
    act(() => useAppStore.getState().selectAccounts(['mock-1']))
    act(() => useAppStore.getState().applyAutomation('ballContinuous', true))
    expect(useAppStore.getState().mock.accounts[0].automations.ballContinuous).toBe(true)
    expect(useAppStore.getState().mock.accounts[1].automations.ballContinuous).toBe(false)
  })
  it('keeps the two ball modes mutually exclusive', () => {
    act(() => useAppStore.getState().setSetting('mockMode', true))
    act(() => useAppStore.getState().selectAccounts(['mock-1']))
    act(() => useAppStore.getState().applyAutomation('ballUntilCapture', true))
    act(() => useAppStore.getState().applyAutomation('ballContinuous', true))
    const automations = useAppStore.getState().mock.accounts[0].automations
    expect(automations.ballContinuous).toBe(true)
    expect(automations.ballUntilCapture).toBe(false)
  })
  it('keeps potion threshold as a UI percentage', () => {
    act(() => useAppStore.getState().setSetting('mockMode', true))
    act(() => useAppStore.getState().selectAccounts(['mock-1']))
    act(() => useAppStore.getState().setPotionThreshold(40))
    expect(useAppStore.getState().mock.accounts[0].potionThreshold).toBe(40)
  })
  it('changes Inventory from items to Pokémon without reloading', () => {
    act(() => useAppStore.getState().setSetting('mockMode', true))
    const host = render()
    const tab = [...host.querySelectorAll('button')].find(
      (button) => button.querySelector('.sidebar-nav-label')?.textContent === 'Inventários',
    )!
    act(() => tab.click())
    const pokemon = [...host.querySelectorAll('button')].find(
      (button) => button.textContent === 'Pokémon',
    )!
    act(() => pokemon.click())
    expect(host.textContent).toContain('Venusaur')
    expect(useAppStore.getState().ui.inventoryTab).toBe('pokemon')
  })
  it('persists the selected startup concurrency preference in the UI state', () => {
    const host = render()
    const settingsNav = [...host.querySelectorAll('button')].find(
      (button) => button.querySelector('.sidebar-nav-label')?.textContent === 'Configurações',
    )!
    act(() => settingsNav.click())

    const options = host.querySelectorAll<HTMLButtonElement>('.startup-concurrency-options button')
    expect(options).toHaveLength(4)
    act(() => options[2].click())

    expect(useAppStore.getState().settings.startupBrowserConcurrency).toBe(3)
    expect(options[2].getAttribute('aria-pressed')).toBe('true')
    expect(host.textContent).toContain('Aplicado na próxima inicialização.')
  })
  it('clears all mock state when demonstration mode is disabled', () => {
    act(() => useAppStore.getState().setSetting('mockMode', true))
    act(() => useAppStore.getState().applyAutomation('ballContinuous', true))
    act(() => useAppStore.getState().setSetting('mockMode', false))
    expect(useAppStore.getState().mock.accounts).toEqual([])
    expect(useAppStore.getState().real.accounts).toEqual([])
  })
  it('derives changing mock metrics only for farming accounts', () => {
    act(() => useAppStore.getState().setSetting('mockMode', true))
    const before = useAppStore.getState().mock.accounts[0].xpPerHour
    act(() => useAppStore.getState().advanceMock())
    const after = useAppStore.getState().mock.accounts[0]
    expect(after.xpPerHour).not.toBe(before)
    expect(after.goldPerHour).toBeGreaterThan(0)
    expect(useAppStore.getState().mock.accounts[3].xpPerHour).toBe(0)
  })
  it('opens the detailed account dashboard from its card', () => {
    act(() => useAppStore.getState().setSetting('mockMode', true))
    const host = render()
    const card = host.querySelector<HTMLButtonElement>(
      'button[aria-label="Abrir detalhes de DemoTrainerTwo"]',
    )!
    act(() => card.click())
    expect(host.querySelector('.shared-page-header h1')?.textContent).toBe('DemoTrainerTwo')
    expect(host.querySelector('.shared-page-header-plaque > img')).not.toBeNull()
    expect(host.querySelector('.shared-page-header .detail-back')?.textContent).toContain(
      'Voltar ao Dashboard',
    )
    expect(host.textContent).toContain('Pokémon ativo')
    expect(host.textContent).toContain('Últimos eventos')
    expect(host.querySelector('.detail-premium-card h2')?.textContent).toBe('Informações da conta')
    expect(host.querySelectorAll('.detail-account-stat')).toHaveLength(4)
    expect(
      [...host.querySelectorAll('.detail-account-info-list dt')].map((label) =>
        label.textContent?.trim(),
      ),
    ).toEqual(['VIP', 'Mapa', 'Hunt'])
    expect(host.querySelector('.detail-account-info-icon-map')).not.toBeNull()
    expect(host.querySelector('.detail-account-info-icon-hunt')).not.toBeNull()
    expect([...host.querySelectorAll('.detail-account-info-icon')].every((icon) => icon.tagName === 'IMG')).toBe(true)
    expect(
      [...host.querySelectorAll('.detail-premium-card h2')].map((heading) => heading.textContent),
    ).not.toContain('Inventário')
  })
  it('keeps real data isolated when the demonstration state is discarded', () => {
    const realAccount = { ...mockAccounts[0], id: 'real-1', nick: 'Conta Real' }
    act(() => useAppStore.setState({ real: { accounts: [realAccount] } }))
    act(() => useAppStore.getState().setSetting('mockMode', true))
    act(() => useAppStore.getState().setSetting('mockMode', false))
    expect(useAppStore.getState().real.accounts[0].nick).toBe('Conta Real')
    expect(useAppStore.getState().mock.accounts).toEqual([])
  })
  it.each(['background', 'browser', 'transitioning'] as const)(
    'keeps Dashboard as the application route for a real %s account',
    (mode) => {
      const account = { ...mockAccounts[0], id: `real-${mode}`, mode, nick: `Conta ${mode}` }
      act(() => useAppStore.setState({ real: { accounts: [account] } }))
      const host = render()
      expect(host.textContent).toContain('Dashboard')
      expect(host.textContent).toContain(`Conta ${mode}`)
      expect(host.textContent).not.toContain('INTEGRAÇÃO REAL')
      if (mode === 'browser') {
        expect(host.querySelector('.premium-account-card .mode-browser .account-mode-mark')).not.toBeNull()
      }
    },
  )
  it('shows reconnecting instead of a stale Browser action after Browser owner loss', () => {
    const account = {
      ...mockAccounts[0],
      id: 'real-reconnecting',
      mode: 'transitioning' as const,
      status: 'connecting' as const,
      runtime: 'reconnecting' as const,
    }
    act(() => useAppStore.setState({ real: { accounts: [account] } }))
    act(() => useAppStore.getState().openAccountDetail(account.id))
    const host = render()
    expect(host.textContent).toContain('Reconectando...')
    expect(host.textContent).not.toContain('Voltar ao background')
  })
  it('offers a safe browser reconnect for an errored Browser account', async () => {
    const account = {
      ...mockAccounts[0],
      id: 'real-browser-error',
      nick: 'DemoTrainerOne',
      mode: 'browser' as const,
      status: 'error' as const,
    }
    invokeMock.mockResolvedValue(undefined)
    act(() => useAppStore.setState({ real: { accounts: [account] } }))
    act(() => useAppStore.getState().openAccountDetail(account.id))
    const host = render()
    const reconnect = [...host.querySelectorAll('button')].find(
      (button) => button.textContent === 'Reconectar navegador',
    )!

    await act(async () => {
      reconnect.click()
      await Promise.resolve()
    })

    expect(invokeMock).toHaveBeenCalledWith('reconnect_browser_control', {
      accountId: account.id,
    })
    expect(host.textContent).not.toContain('Voltar ao background')
  })
  it('shows the reason when a browser reconnect is rejected', async () => {
    const account = {
      ...mockAccounts[0],
      id: 'real-browser-error',
      nick: 'DemoTrainerOne',
      mode: 'browser' as const,
      status: 'error' as const,
    }
    invokeMock.mockRejectedValue('Reconexão falhou: PID do navegador não confirmado.')
    act(() => useAppStore.setState({ real: { accounts: [account] } }))
    act(() => useAppStore.getState().openAccountDetail(account.id))
    const host = render()
    const reconnect = [...host.querySelectorAll('button')].find(
      (button) => button.textContent === 'Reconectar navegador',
    )!

    await act(async () => {
      reconnect.click()
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(host.textContent).toContain('Reconexão falhou: PID do navegador não confirmado.')
  })
  it('keeps the detail route independent from Browser lifecycle state', () => {
    const account = {
      ...mockAccounts[0],
      id: 'real-browser',
      mode: 'browser' as const,
      nick: 'DemoTrainerOne',
    }
    act(() => useAppStore.setState({ real: { accounts: [account] } }))
    act(() => useAppStore.getState().openAccountDetail('real-browser'))
    const host = render()
    expect(host.querySelector('.shared-page-header h1')?.textContent).toBe('DemoTrainerOne')
    expect(host.textContent).toContain('Voltar ao background')
    const dashboard = [...host.querySelectorAll('button')].find(
      (button) => button.querySelector('.sidebar-nav-label')?.textContent === 'Dashboard',
    )!
    act(() => dashboard.click())
    expect(host.textContent).toContain('Dashboard')
    expect(host.textContent).toContain('DemoTrainerOne')
  })
  it.each(['background', 'browser'] as const)(
    'assigns the intended color role to account actions in %s mode',
    (mode) => {
      const account = { ...mockAccounts[0], id: `detail-colors-${mode}`, mode }
      act(() => useAppStore.setState({ real: { accounts: [account] } }))
      act(() => useAppStore.getState().openAccountDetail(account.id))
      const host = render()

      expect(host.querySelector('.detail-mode .account-mode-mark')).not.toBeNull()
      expect(host.querySelector('.account-detail-header-actions .danger-button')?.textContent)
        .toContain('Remover conta')
      if (mode === 'background') {
        expect(host.querySelector('.detail-open-browser')).not.toBeNull()
        expect(host.querySelector('.detail-return-background')).toBeNull()
      } else {
        expect(host.querySelector('.detail-return-background')).not.toBeNull()
        expect(host.querySelector('.detail-open-browser')).toBeNull()
      }
    },
  )
  it('offers a login action only for the account whose profile needs authentication', () => {
    const account = {
      ...mockAccounts[0],
      id: 'real-login',
      nick: 'Conta expirada',
      status: 'login_required' as const,
      mode: 'browser' as const,
    }
    act(() => useAppStore.setState({ real: { accounts: [account] } }))
    act(() => useAppStore.getState().openAccountDetail(account.id))
    const host = render()
    expect(
      [...host.querySelectorAll('button')].some((button) => button.textContent === 'Fazer login'),
    ).toBe(true)
    expect(
      [...host.querySelectorAll('button')].some((button) => button.textContent === 'Remover conta'),
    ).toBe(true)
  })
  it('shows only server-provided Hunt options in the Hunt controller', () => {
    const account = {
      ...mockAccounts[0],
      id: 'real-hunt',
      nick: 'DemoTrainerOne',
      huntOptions: [{ slug: 'shellder', name: 'Shellder' }],
    }
    act(() => useAppStore.setState({ real: { accounts: [account] } }))
    act(() => useAppStore.getState().selectAccounts(['real-hunt']))
    act(() => useAppStore.getState().setPage('Automações'))
    const host = render()
    expect(host.textContent).toContain('CONTROLE DE HUNT')
    const huntSearch = host.querySelector<HTMLInputElement>('input[aria-label="Buscar Hunt"]')!
    expect(huntSearch.value).toBe('Shellder')
    act(() => huntSearch.focus())
    expect(host.querySelector('[role="listbox"]')?.textContent).toContain('Shellder')
    expect(host.querySelector('[role="listbox"]')?.textContent).not.toContain('Lapras')
  })
  it('shows the union of Auto Buy items and marks differing values instead of favoring one account', () => {
    const first = {
      ...mockAccounts[0],
      id: 'auto-union-a',
      nick: 'Conta A',
      potionIds: ['204', '202'],
      autoBuyRules: [
        { kind: 'item' as const, itemId: 204, minimum: 100, quantity: 10, enabled: false },
      ],
    }
    const second = {
      ...mockAccounts[1],
      id: 'auto-union-b',
      nick: 'Conta B',
      potionIds: ['203', '204'],
      autoBuyRules: [
        { kind: 'item' as const, itemId: 204, minimum: 200, quantity: 12, enabled: false },
      ],
    }
    act(() => {
      useAppStore.setState({ real: { accounts: [first, second] } })
      useAppStore.getState().selectAccounts([first.id, second.id])
      useAppStore.getState().setPage('Automações')
    })
    const host = render()
    const names = [...host.querySelectorAll('.auto-buy-item-name')].map(
      (node) => node.textContent ?? '',
    )
    expect(names.some((name) => name.includes('Ultimate Potion'))).toBe(true)
    expect(names.some((name) => name.includes('Ultra Potion'))).toBe(true)
    expect(names.some((name) => name.includes('Hyper Potion'))).toBe(true)

    const shared = [...host.querySelectorAll<HTMLElement>('.auto-buy-item')].find((node) =>
      node.querySelector('.auto-buy-item-name')?.textContent?.includes('Ultimate Potion'),
    )!
    expect(
      shared.querySelector('[aria-label="Estoque mínimo Ultimate Potion (MIXED, vários valores)"]'),
    ).not.toBeNull()
    expect(
      shared.querySelector('[aria-label="Quantidade Ultimate Potion (MIXED, vários valores)"]'),
    ).not.toBeNull()
    expect(shared.textContent).toContain('Vários valores')

    const onlyFirst = [...host.querySelectorAll<HTMLElement>('.auto-buy-item')].find((node) =>
      node.querySelector('.auto-buy-item-name')?.textContent?.includes('Ultra Potion'),
    )!
    expect(onlyFirst.textContent).toContain('Não configurado em: Conta B')
    const onlySecond = [...host.querySelectorAll<HTMLElement>('.auto-buy-item')].find((node) =>
      node.querySelector('.auto-buy-item-name')?.textContent?.includes('Hyper Potion'),
    )!
    expect(onlySecond.textContent).toContain('Não configurado em: Conta A')
  })
  it('shows a shared Auto Buy value when selected accounts have identical rules', () => {
    const first = {
      ...mockAccounts[0],
      id: 'auto-same-a',
      potionIds: ['204'],
      autoBuyRules: [
        { kind: 'item' as const, itemId: 204, minimum: 120, quantity: 8, enabled: false },
      ],
    }
    const second = {
      ...mockAccounts[1],
      id: 'auto-same-b',
      potionIds: ['204'],
      autoBuyRules: [
        { kind: 'item' as const, itemId: 204, minimum: 120, quantity: 8, enabled: false },
      ],
    }
    act(() => {
      useAppStore.setState({ real: { accounts: [first, second] } })
      useAppStore.getState().selectAccounts([first.id, second.id])
      useAppStore.getState().setPage('Automações')
    })
    const host = render()
    expect(
      host.querySelector<HTMLInputElement>('input[aria-label="Estoque mínimo Ultimate Potion"]')
        ?.value,
    ).toBe('120')
    expect(
      host.querySelector<HTMLInputElement>('input[aria-label="Quantidade Ultimate Potion"]')?.value,
    ).toBe('8')
    expect(host.querySelector('.auto-buy-mixed')).toBeNull()
  })
  it('applies an edited Auto Buy field to every compatible account and preserves each other field', async () => {
    const first = {
      ...mockAccounts[0],
      id: 'auto-update-a',
      nick: 'Conta A',
      potionIds: ['204'],
      autoBuyRules: [
        { kind: 'item' as const, itemId: 204, minimum: 100, quantity: 10, enabled: false },
      ],
    }
    const second = {
      ...mockAccounts[1],
      id: 'auto-update-b',
      nick: 'Conta B',
      potionIds: ['204'],
      autoBuyRules: [
        { kind: 'item' as const, itemId: 204, minimum: 200, quantity: 12, enabled: false },
      ],
    }
    invokeMock.mockResolvedValue(undefined)
    act(() => {
      useAppStore.setState({ real: { accounts: [first, second] } })
      useAppStore.getState().selectAccounts([first.id, second.id])
      useAppStore.getState().setPage('Automações')
    })
    const host = render()
    const input = host.querySelector<HTMLInputElement>(
      'input[aria-label="Estoque mínimo Ultimate Potion (MIXED, vários valores)"]',
    )!
    const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!
    act(() => {
      setValue.call(input, '350')
      input.dispatchEvent(new Event('input', { bubbles: true }))
    })
    expect(input.value).toBe('350')

    // Simulate a runtime poll arriving while the user still has an uncommitted draft.
    act(() =>
      useAppStore.setState({
        real: {
          accounts: [
            { ...first, autoBuyRules: [{ ...first.autoBuyRules[0], minimum: 125 }] },
            second,
          ],
        },
      }),
    )
    expect(input.value).toBe('350')

    await act(async () => {
      input.focus()
      input.blur()
      await Promise.resolve()
      await Promise.resolve()
    })
    const calls = invokeMock.mock.calls.filter(([command]) => command === 'update_auto_buy_rule')
    expect(calls).toHaveLength(2)
    expect(calls.map(([, args]) => args)).toEqual([
      { accountId: first.id, kind: 'item', itemId: 204, minimum: 350, quantity: 10 },
      { accountId: second.id, kind: 'item', itemId: 204, minimum: 350, quantity: 12 },
    ])
    expect(host.textContent).toContain('Atualização enviada para 2 conta(s)')
    act(() =>
      useAppStore.setState({
        real: {
          accounts: [
            { ...first, autoBuyRules: [{ ...first.autoBuyRules[0], minimum: 350 }] },
            { ...second, autoBuyRules: [{ ...second.autoBuyRules[0], minimum: 350 }] },
          ],
        },
      }),
    )
    const confirmed = host.querySelector<HTMLInputElement>(
      'input[aria-label="Estoque mínimo Ultimate Potion"]',
    )!
    expect(confirmed.value).toBe('350')
  })
  it('does not reuse an Auto Buy draft after the selected account set changes', () => {
    const first = {
      ...mockAccounts[0],
      id: 'auto-switch-a',
      nick: 'Conta A',
      potionIds: ['204'],
      autoBuyRules: [
        { kind: 'item' as const, itemId: 204, minimum: 100, quantity: 10, enabled: false },
      ],
    }
    const second = {
      ...mockAccounts[1],
      id: 'auto-switch-b',
      nick: 'Conta B',
      potionIds: ['204'],
      autoBuyRules: [
        { kind: 'item' as const, itemId: 204, minimum: 220, quantity: 15, enabled: false },
      ],
    }
    act(() => {
      useAppStore.setState({ real: { accounts: [first, second] } })
      useAppStore.getState().selectAccounts([first.id])
      useAppStore.getState().setPage('Automações')
    })
    const host = render()
    const input = host.querySelector<HTMLInputElement>(
      'input[aria-label="Estoque mínimo Ultimate Potion"]',
    )!
    const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!
    act(() => {
      setValue.call(input, '333')
      input.dispatchEvent(new Event('input', { bubbles: true }))
      useAppStore.getState().selectAccounts([second.id])
    })
    const switchedInput = host.querySelector<HTMLInputElement>(
      'input[aria-label="Estoque mínimo Ultimate Potion"]',
    )!
    expect(switchedInput.value).toBe('220')
    expect(invokeMock.mock.calls.some(([command]) => command === 'update_auto_buy_rule')).toBe(
      false,
    )
  })
  it('reports partial Auto Buy update failures and only invokes compatible accounts', async () => {
    const first = {
      ...mockAccounts[0],
      id: 'auto-partial-a',
      nick: 'Conta A',
      potionIds: ['204'],
      autoBuyRules: [
        { kind: 'item' as const, itemId: 204, minimum: 100, quantity: 10, enabled: false },
      ],
    }
    const second = {
      ...mockAccounts[1],
      id: 'auto-partial-b',
      nick: 'Conta B',
      potionIds: ['204', '203'],
      autoBuyRules: [
        { kind: 'item' as const, itemId: 204, minimum: 100, quantity: 10, enabled: false },
      ],
    }
    invokeMock.mockImplementation((_command: string, args: { accountId: string }) =>
      args.accountId === first.id
        ? Promise.reject(new Error('IPC indisponível'))
        : Promise.resolve(undefined),
    )
    act(() => {
      useAppStore.setState({ real: { accounts: [first, second] } })
      useAppStore.getState().selectAccounts([first.id, second.id])
      useAppStore.getState().setPage('Automações')
    })
    const host = render()
    const onlySecond = [...host.querySelectorAll<HTMLElement>('.auto-buy-item')].find((node) =>
      node.querySelector('.auto-buy-item-name')?.textContent?.includes('Hyper Potion'),
    )!
    expect(onlySecond.textContent).toContain('Não configurado em: Conta A')
    const shared = [...host.querySelectorAll<HTMLElement>('.auto-buy-item')].find((node) =>
      node.querySelector('.auto-buy-item-name')?.textContent?.includes('Ultimate Potion'),
    )!
    const input = shared.querySelector<HTMLInputElement>(
      'input[aria-label="Estoque mínimo Ultimate Potion"]',
    )!
    const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!
    await act(async () => {
      setValue.call(input, '400')
      input.dispatchEvent(new Event('input', { bubbles: true }))
      input.focus()
      input.blur()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(
      invokeMock.mock.calls.filter(([command]) => command === 'update_auto_buy_rule'),
    ).toHaveLength(2)
    expect(shared.textContent).toContain('Atualizado em 1; falhou em Conta A.')
  })
  it('filters the Hunt search and chooses the clicked server-provided Hunt', () => {
    const account = {
      ...mockAccounts[0],
      id: 'hunt-search',
      huntOptions: [
        { slug: 'shellder', name: 'Shellder' },
        { slug: 'seel', name: 'Seel' },
      ],
    }
    act(() => useAppStore.setState({ real: { accounts: [account] } }))
    act(() => useAppStore.getState().selectAccounts([account.id]))
    act(() => useAppStore.getState().setPage('Automações'))
    const host = render()
    const huntSearch = host.querySelector<HTMLInputElement>('input[aria-label="Buscar Hunt"]')!

    act(() => huntSearch.focus())
    const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!
    act(() => {
      setValue.call(huntSearch, 'seel')
      huntSearch.dispatchEvent(new Event('input', { bubbles: true }))
    })
    expect(host.querySelector('[role="listbox"]')?.textContent).toContain('Seel')
    expect(host.querySelector('[role="listbox"]')?.textContent).not.toContain('Shellder')

    const option = [...host.querySelectorAll<HTMLButtonElement>('[role="option"]')].find(
      (button) => button.textContent === 'Seel',
    )!
    act(() => option.click())
    expect(huntSearch.value).toBe('Seel')
    expect(host.querySelector('[role="listbox"]')).toBeNull()
  })
  it('keeps Manager commands available for a Browser owner with a ready transport', () => {
    const account = {
      ...mockAccounts[0],
      id: 'real-browser-command',
      mode: 'browser' as const,
      commandTransportAvailable: true,
      huntOptions: [{ slug: 'shellder', name: 'Shellder' }],
    }
    act(() => useAppStore.setState({ real: { accounts: [account] } }))
    act(() => useAppStore.getState().selectAccounts([account.id]))
    act(() => useAppStore.getState().setPage('Automações'))
    const host = render()
    const huntButton = [...host.querySelectorAll('button')].find(
      (button) => button.textContent === 'Iniciar Hunt',
    )!
    expect(huntButton.disabled).toBe(false)
    expect(
      host.querySelector<HTMLButtonElement>(
        'button[role="switch"][aria-label="Ativar Usar Poções"]',
      )?.disabled,
    ).toBe(false)
  })
  it('shows command controls as unavailable only while transport is unavailable', () => {
    const account = {
      ...mockAccounts[0],
      id: 'real-transition-command',
      mode: 'transitioning' as const,
      commandTransportAvailable: false,
      huntOptions: [{ slug: 'shellder', name: 'Shellder' }],
    }
    act(() => useAppStore.setState({ real: { accounts: [account] } }))
    act(() => useAppStore.getState().selectAccounts([account.id]))
    act(() => useAppStore.getState().setPage('Automações'))
    const host = render()
    expect(host.textContent).toContain('Aguardando transporte...')
    expect(
      host.querySelector<HTMLButtonElement>(
        'button[role="switch"][aria-label="Ativar Usar Poções"]',
      )?.disabled,
    ).toBe(true)
  })
  it('shows visual combat preferences and preserves differing multi-account selections until an explicit edit', () => {
    const first = {
      ...mockAccounts[0],
      id: 'real-preference-a',
      nick: 'Conta A',
      potionIds: ['204', '202'],
      ballIds: ['4', '3'],
    }
    const second = {
      ...mockAccounts[1],
      id: 'real-preference-b',
      nick: 'Conta B',
      potionIds: ['203'],
      ballIds: ['4'],
    }
    act(() => useAppStore.setState({ real: { accounts: [first, second] } }))
    act(() => useAppStore.getState().selectAccounts([first.id, second.id]))
    act(() => useAppStore.getState().setPage('Automações'))
    const host = render()
    expect(host.textContent).toContain('SELEÇÃO DE POÇÕES')
    expect(host.textContent).toContain('SELEÇÃO DE BALLS')
    expect(host.textContent).toContain('Ultimate Potion')
    expect(host.textContent).toContain('Ultra Ball')
    expect(host.textContent).toContain('Seleções diferentes entre contas')
  })
  it('gives Automations an internal, keyboard-focusable scroll region', () => {
    act(() => useAppStore.getState().setSetting('mockMode', true))
    act(() => useAppStore.getState().setPage('Automações'))
    const host = render()
    const viewport = host.querySelector<HTMLElement>('[aria-label="Conteúdo de Automações"]')
    expect(viewport?.classList.contains('page-scroll')).toBe(true)
    expect(viewport?.getAttribute('role')).toBe('region')
    expect(viewport?.tabIndex).toBe(0)
  })
  it('keeps the Automation viewport stable while every shared Switch is toggled repeatedly', () => {
    act(() => useAppStore.getState().setSetting('mockMode', true))
    act(() => useAppStore.getState().selectAccounts(['mock-1']))
    act(() => useAppStore.getState().setPage('Automações'))
    const host = render()
    const viewport = host.querySelector<HTMLDivElement>('[aria-label="Conteúdo de Automações"]')!
    const route = host.querySelector<HTMLElement>('.route-viewport')!
    const page = viewport.querySelector<HTMLElement>('.automation-page')!
    const main = route.closest<HTMLElement>('.content')!
    const rect = {
      x: 238,
      y: 0,
      top: 0,
      left: 238,
      right: 1440,
      bottom: 840,
      width: 1202,
      height: 840,
      toJSON: () => ({}),
    }
    Object.defineProperties(viewport, {
      clientHeight: { value: 840, configurable: true },
      scrollTop: { value: 260, writable: true, configurable: true },
      scrollHeight: { get: () => 1_480, configurable: true },
    })
    Object.defineProperties(route, {
      scrollTop: { value: 0, writable: true, configurable: true },
    })
    Object.defineProperties(main, {
      scrollTop: { value: 0, writable: true, configurable: true },
    })
    vi.spyOn(viewport, 'getBoundingClientRect').mockReturnValue(rect)
    vi.spyOn(main, 'getBoundingClientRect').mockReturnValue(rect)
    vi.spyOn(route, 'getBoundingClientRect').mockReturnValue(rect)
    vi.spyOn(page, 'getBoundingClientRect').mockReturnValue(rect)
    const before = {
      element: viewport,
      clientHeight: viewport.clientHeight,
      scrollTop: viewport.scrollTop,
      viewportRect: viewport.getBoundingClientRect(),
      mainRect: main.getBoundingClientRect(),
      routeRect: route.getBoundingClientRect(),
      pageRect: page.getBoundingClientRect(),
    }
    const labels = [
      'Ativar Usar Poções',
      'Ativar Usar Revive ao desmaiar',
      'Ativar Lançar até Capturar',
      'Ativar Lançar sem Parar',
      'Ativar Venda Automática',
      'Ativar Voltar à hunt ao morrer/reset',
      'Ativar Compra Automática de Poções',
      'Ativar Compra Automática de Balls',
    ]
    expect(host.querySelector('input[type="checkbox"][aria-label^="Ativar"]')).toBeNull()
    for (const label of labels) {
      for (let attempt = 0; attempt < 10; attempt += 1) {
        const toggle = host.querySelector<HTMLButtonElement>(
          `button[role="switch"][aria-label="${label}"]`,
        )
        if (!toggle) throw new Error(`Switch ausente: ${label}`)
        act(() => toggle.click())
        expect(host.querySelector('[aria-label="Conteúdo de Automações"]')).toBe(before.element)
        expect(viewport.clientHeight).toBe(before.clientHeight)
        expect(viewport.getBoundingClientRect().top).toBe(before.viewportRect.top)
        expect(viewport.getBoundingClientRect().bottom).toBe(before.viewportRect.bottom)
        expect(viewport.scrollTop).toBe(before.scrollTop)
        expect(route.scrollTop).toBe(0)
        expect(main.scrollTop).toBe(0)
        expect(main.getBoundingClientRect().top).toBe(before.mainRect.top)
        expect(main.getBoundingClientRect().bottom).toBe(before.mainRect.bottom)
        expect(route.getBoundingClientRect().top).toBe(before.routeRect.top)
        expect(route.getBoundingClientRect().bottom).toBe(before.routeRect.bottom)
        expect(page.getBoundingClientRect().top).toBe(before.pageRect.top)
        expect(route.contains(viewport)).toBe(true)
        expect(viewport.contains(page)).toBe(true)
      }
    }
    expect(viewport.scrollHeight).toBeGreaterThan(viewport.clientHeight)
    viewport.scrollTop = 0
    expect(viewport.textContent).toContain('Automações')
    expect(viewport.textContent).toContain('Selecionar todas')
    expect(viewport.textContent).toContain('CONTROLE DE HUNT')
  })
  it('does not show implementation-status copy inside automatic purchase cards', () => {
    act(() => useAppStore.getState().setSetting('mockMode', true))
    act(() => useAppStore.getState().selectAccounts(['mock-1']))
    act(() => useAppStore.getState().setPage('Automações'))
    const host = render()
    expect(host.textContent).toContain('Compra Automática de Poções')
    expect(host.textContent).toContain('Compra Automática de Balls')
    expect(host.textContent).not.toContain('Modo estoque')
    expect(host.textContent).not.toContain('Estoque reconciliado')
  })
  it('organizes Automations into the premium header, selection, control and grid sections', () => {
    act(() => useAppStore.getState().setSetting('mockMode', true))
    act(() => useAppStore.getState().selectAccounts(['mock-1']))
    act(() => useAppStore.getState().setPage('Automações'))
    const host = render()

    expect(host.querySelector('.shared-page-header h1')?.textContent).toBe('Automações')
    expect(host.querySelector('.automation-account-bar')).not.toBeNull()
    expect(host.querySelector('.hunt-control')).not.toBeNull()
    expect(host.querySelector('.combat-item-options-potion')).not.toBeNull()
    expect(host.querySelector('.combat-item-options-ball')).not.toBeNull()
    expect(host.querySelectorAll('.automation-section .automation-card')).toHaveLength(8)
  })
  it('uses compact item choices with consolidated stock and only known current-Hunt types', () => {
    act(() => useAppStore.getState().setSetting('mockMode', true))
    act(() => useAppStore.getState().selectAccounts(['mock-1']))
    act(() => useAppStore.getState().setPage('Automações'))
    const host = render()
    const preferences = host.querySelector('.combat-preferences')!

    expect(host.querySelector('.hunt-current')?.textContent).toContain('Ancient Pupitar')
    expect(host.querySelector('.hunt-type-badges')?.textContent).toContain('ROCK')
    expect(host.querySelector('.hunt-type-badges')?.textContent).toContain('GROUND')
    expect(preferences.querySelectorAll('button[role="checkbox"]')).toHaveLength(10)
    expect(preferences.querySelector('.priority-controls')).toBeNull()
    expect(preferences.textContent).not.toContain('DemoTrainerTwo:')
    expect(preferences.textContent).not.toContain('A ordem é a prioridade enviada ao jogo')
    expect(preferences.textContent).not.toContain('A poção exibida no Dashboard só muda')
    expect(host.querySelectorAll('.automation-card-icon')).toHaveLength(8)
  })
  it('keeps Hunt schedulable during combat and updates its deadline display', () => {
    vi.useFakeTimers()
    vi.setSystemTime(1_000)
    try {
      const account = {
        ...mockAccounts[0],
        id: 'real-locked-hunt',
        mode: 'background' as const,
        huntOptions: [{ slug: 'shellder', name: 'Shellder' }],
        combatLockUntil: 4_000,
        serverOffsetMs: 0,
      }
      act(() => useAppStore.setState({ real: { accounts: [account] } }))
      act(() => useAppStore.getState().selectAccounts([account.id]))
      act(() => useAppStore.getState().setPage('Automações'))
      const host = render()
      const button = [...host.querySelectorAll('button')].find(
        (candidate) => candidate.textContent === 'Agendar Hunt',
      )!
      expect(button.disabled).toBe(false)
      act(() => vi.advanceTimersByTime(1_000))
      act(() =>
        useAppStore.setState({ real: { accounts: [{ ...account, combatLockUntil: 6_000 }] } }),
      )
      expect(button.textContent).toBe('Agendar Hunt')
      act(() => vi.advanceTimersByTime(4_000))
      expect(button.disabled).toBe(false)
      expect(button.textContent).toBe('Iniciar Hunt')
    } finally {
      vi.useRealTimers()
    }
  })
  it('shows a cancellable pending Center navigation without disabling the destination controls', () => {
    const account = {
      ...mockAccounts[0],
      id: 'real-pending-center',
      mode: 'browser' as const,
      commandTransportAvailable: true,
      huntOptions: [{ slug: 'shellder', name: 'Shellder' }],
      combatLockUntil: Date.now() + 5_000,
      serverOffsetMs: 0,
      pendingNavigation: { kind: 'center' as const },
    }
    act(() => useAppStore.setState({ real: { accounts: [account] } }))
    act(() => useAppStore.getState().selectAccounts([account.id]))
    act(() => useAppStore.getState().setPage('Automações'))
    const host = render()
    expect(host.textContent).toContain('Aguardando: Pokémon Center')
    expect(
      [...host.querySelectorAll('button')].find(
        (button) => button.textContent === 'Voltar ao Center',
      )?.disabled,
    ).toBe(false)
    expect(
      [...host.querySelectorAll('button')].some((button) => button.textContent === 'Cancelar'),
    ).toBe(true)
  })
  it('shows the missing-Brave dialog without creating an account; Cancel leaves the Dashboard empty', async () => {
    setupDesktop((command) => command === 'start_real_account' ? Promise.resolve({ status: 'browserNotFound' }) : Promise.resolve(null))
    const host = render()
    await flushDesktopEffects()
    const add = [...host.querySelectorAll('button')].find((button) => button.textContent === '+ Adicionar conta')!
    await act(async () => { add.click(); await Promise.resolve() })
    expect(host.querySelector('[role="dialog"]')?.textContent).toContain('Navegador necessário')
    expect(host.querySelector('[role="dialog"]')?.textContent).toContain('O Brave não foi encontrado neste computador.')
    expect(useAppStore.getState().real.accounts).toHaveLength(0)
    expect(host.querySelectorAll('.premium-account-card')).toHaveLength(0)
    expect(invokeMock.mock.calls.filter(([command]) => command === 'start_real_account')).toHaveLength(1)
    await act(async () => host.querySelector<HTMLButtonElement>('[role="dialog"] button:last-child')!.click())
    expect(host.querySelector('[role="dialog"]')).toBeNull()
    expect(useAppStore.getState().real.accounts).toHaveLength(0)
  })
  it('uses the fixed Brave download command and retries the atomic add flow without restarting', async () => {
    let attempts = 0
    setupDesktop((command) => {
      if (command === 'start_real_account') {
        attempts += 1
        return Promise.resolve(attempts < 3
          ? { status: 'browserNotFound' }
          : { status: 'started', accountId: 'started-account' })
      }
      return Promise.resolve(null)
    })
    const host = render()
    await flushDesktopEffects()
    const add = [...host.querySelectorAll('button')].find((button) => button.textContent === '+ Adicionar conta')!
    await act(async () => { add.click(); await Promise.resolve() })
    await act(async () => {
      [...host.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === 'Baixar Brave')!.click()
      await Promise.resolve()
    })
    expect(invokeMock.mock.calls.filter(([command]) => command === 'open_brave_download_page')).toHaveLength(1)
    await act(async () => {
      [...host.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === 'Tentar novamente')!.click()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="dialog"]')?.textContent).toContain('Brave ainda não foi encontrado.')
    await act(async () => {
      [...host.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === 'Tentar novamente')!.click()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="dialog"]')).toBeNull()
    expect(attempts).toBe(3)
  })
  it('prevents duplicate add-account commands while the first request is pending', async () => {
    vi.useFakeTimers()
    let currentSnapshot = { accounts: [], diagnostic: { lifecycle: 'closed', state: 'Aguardando', message: '', accountId: null, welcomeReceived: false } } as unknown as IntegrationSnapshot
    let resolveInitialSnapshot: ((snapshot: IntegrationSnapshot) => void) | undefined
    let snapshotRequests = 0
    let resolveStart: ((result: { status: 'started'; accountId: string }) => void) | undefined
    setupDesktop(
      (command) => command === 'start_real_account'
        ? new Promise((resolve) => { resolveStart = resolve })
        : Promise.resolve(null),
      () => {
        snapshotRequests += 1
        return snapshotRequests === 1
          ? new Promise((resolve) => { resolveInitialSnapshot = resolve })
          : currentSnapshot
      },
    )
    const host = render()
    await flushDesktopEffects()
    const add = [...host.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === '+ Adicionar conta')!
    act(() => { add.click(); add.click() })
    expect(invokeMock.mock.calls.filter(([command]) => command === 'start_real_account')).toHaveLength(1)
    expect(add.disabled).toBe(true)
    await act(async () => { resolveStart?.({ status: 'started', accountId: 'pending-account' }) })
    expect(add.disabled).toBe(true)
    currentSnapshot = { accounts: [], diagnostic: { lifecycle: 'authenticated', state: 'Conectado', message: '', accountId: 'pending-account', welcomeReceived: true } } as unknown as IntegrationSnapshot
    await act(async () => {
      resolveInitialSnapshot?.(currentSnapshot)
      await Promise.resolve()
    })
    expect(add.disabled).toBe(true)
    await act(async () => { await vi.advanceTimersByTimeAsync(1_200) })
    expect([...host.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === '+ Adicionar conta')?.disabled).toBe(false)
  })
  it('shows a retryable error when a started account fails before authentication', async () => {
    vi.useFakeTimers()
    let currentSnapshot = { accounts: [], diagnostic: { lifecycle: 'closed', state: 'Aguardando', message: '', accountId: null, welcomeReceived: false } } as unknown as IntegrationSnapshot
    let starts = 0
    setupDesktop((command) => {
      if (command === 'start_real_account') {
        starts += 1
        return Promise.resolve({ status: 'started', accountId: starts === 1 ? 'failed-account' : 'retry-account' })
      }
      return Promise.resolve(null)
    }, () => currentSnapshot)
    const host = render()
    await flushDesktopEffects()
    const add = [...host.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === '+ Adicionar conta')!
    await act(async () => { add.click(); await Promise.resolve() })
    expect(add.disabled).toBe(true)

    currentSnapshot = { accounts: [], diagnostic: { lifecycle: 'error', state: 'Erro', message: 'Não foi possível conectar ao CDP.', accountId: 'failed-account', welcomeReceived: false } } as unknown as IntegrationSnapshot
    await act(async () => { await vi.advanceTimersByTimeAsync(1_200) })
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('Não foi possível conectar ao CDP.')
    const retry = [...host.querySelectorAll<HTMLButtonElement>('[role="alert"] button')].find((button) => button.textContent === 'Tentar novamente')!
    expect(retry).toBeDefined()
    expect([...host.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === '+ Adicionar conta')?.disabled).toBe(false)
    await act(async () => { retry.click(); await Promise.resolve() })
    expect(starts).toBe(2)
    expect([...host.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === 'Abrindo navegador…')?.disabled).toBe(true)
  })
  it('does not unlock on another account’s waiting-login runtime, but accepts matching ready-to-login diagnostics', async () => {
    vi.useFakeTimers()
    let starts = 0
    let currentSnapshot = { accounts: [], diagnostic: { lifecycle: 'closed', state: 'Aguardando', message: '', accountId: null, welcomeReceived: false } } as unknown as IntegrationSnapshot
    const accountSnapshot = (id: string, diagnosticAccountId = 'some-other-account'): IntegrationSnapshot => ({
      accounts: [{
        account: { id, nick: 'Conta nova', card_color: '#3b82f6', status: 'connecting', mode: 'browser', runtime: 'waiting_for_login' },
        state: { no_centro: false, items: {}, balls: {}, pokemon: [], automation: { potion_ids: [], ball_ids: [], revive_ids: [] } },
        metrics: { xp_per_hour: 0, gold_per_hour: 0, kills: 0, captures: 0 },
      }],
      diagnostic: { lifecycle: 'waitingForLogin', sessionMode: 'interactive', braveFound: true, profileCreated: true, accountPersisted: true, persistentProfile: true, braveStarted: true, cdpPort: 0, cdpEndpointAvailable: true, cdpEndpointAttempts: 1, cdpEndpointLastError: null, browserProduct: 'Brave', browserWsUrlObtained: true, browserWsConnected: true, cdpConnected: true, targetFound: true, targetIdFound: true, sessionCreated: true, managedGamePage: true, pageEnabled: true, pageNavigateSent: true, gameUrlNavigated: true, finalUrl: null, networkEnabled: true, websocketCount: 0, pokeidleSocketDetected: false, websocketDetected: false, helloDetected: false, welcomeReceived: false, wsUrlCaptured: false, sessionMaterialCaptured: false, rustWsConnected: false, rustHelloSent: false, rustWelcomeReceived: false, browserClosed: false, backgroundActive: false, controlledBravePid: null, automaticReloadUsed: false, nick: null, state: 'Aguardando login', message: '', accountId: diagnosticAccountId },
    } as unknown as IntegrationSnapshot)
    setupDesktop((command) => {
      if (command === 'start_real_account') {
        starts += 1
        return Promise.resolve({ status: 'started', accountId: `pending-${starts}` })
      }
      return Promise.resolve(null)
    }, () => currentSnapshot)
    const host = render()
    await flushDesktopEffects()
    const add = () => [...host.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === '+ Adicionar conta' || button.textContent === 'Abrindo navegador…')!
    await act(async () => { add().click(); await Promise.resolve() })
    expect(add().disabled).toBe(true)

    currentSnapshot = accountSnapshot('pending-1')
    await act(async () => { await vi.advanceTimersByTimeAsync(1_200) })
    expect(add().disabled).toBe(true)
    currentSnapshot = accountSnapshot('pending-1', 'pending-1')
    await act(async () => { await vi.advanceTimersByTimeAsync(1_200) })
    expect(add().disabled).toBe(false)
    expect(host.querySelector('[role="alert"]')).toBeNull()

    await act(async () => { add().click(); await Promise.resolve() })
    expect(add().disabled).toBe(true)
    currentSnapshot = { accounts: [], diagnostic: { ...accountSnapshot('another-account').diagnostic, accountId: 'another-account', lifecycle: 'waitingForLogin', state: 'Aguardando login' } } as unknown as IntegrationSnapshot
    await act(async () => { await vi.advanceTimersByTimeAsync(1_200) })
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('A conta não apareceu na atualização do navegador.')
    expect([...host.querySelectorAll<HTMLButtonElement>('[role="alert"] button')].some((button) => button.textContent === 'Tentar novamente')).toBe(true)
    expect(starts).toBe(2)
  })
  it('keeps an existing account/profile when Brave is missing and retries its browser action', async () => {
    let attempts = 0
    setupDesktop((command) => {
      if (command === 'open_account_browser') {
        attempts += 1
        return attempts === 1 ? Promise.reject(new Error('BROWSER_NOT_FOUND: missing')) : Promise.resolve(null)
      }
      return Promise.resolve(null)
    })
    const host = render()
    await flushDesktopEffects()
    const account = { ...mockAccounts[0], id: 'existing-profile', mode: 'background' as const }
    act(() => {
      useAppStore.getState().setRealAccounts([account])
      useAppStore.getState().openAccountDetail(account.id)
    })
    const open = [...host.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === 'Abrir navegador')!
    await act(async () => { open.click(); await Promise.resolve() })
    expect(host.querySelector('[role="dialog"]')).not.toBeNull()
    expect(useAppStore.getState().real.accounts[0]?.id).toBe('existing-profile')
    await act(async () => {
      [...host.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === 'Tentar novamente')!.click()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="dialog"]')).toBeNull()
    expect(attempts).toBe(2)
    expect(useAppStore.getState().real.accounts[0]?.id).toBe('existing-profile')
  })
})
