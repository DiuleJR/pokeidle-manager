import { create } from 'zustand'
import type { HuntReference } from '../hunts/calculator'
import { mockAccounts } from '../mocks/accounts'
import type {
  AccountDomain,
  AccountDomainCacheEntry,
  AccountDomainResponse,
  AccountView,
  AppSettings,
  AutomationKind,
  Page,
} from '../types'

export type AccountDomainCache = Partial<{
  [D in AccountDomain]: AccountDomainCacheEntry<D>
}>

export interface RealAccountState {
  accounts: AccountView[]
  domains?: Record<string, AccountDomainCache>
}
export interface MockAccountState {
  accounts: AccountView[]
  tick: number
}
export interface UIState {
  page: Page
  selectedIds: string[]
  inventoryTab: 'items' | 'pokemon'
  detailAccountId: string | null
  whereToHuntAccountId: string | null
}

export interface AppState {
  real: RealAccountState
  mock: MockAccountState
  whereHuntReference: HuntReference | null
  ui: UIState
  settings: AppSettings
  setPage: (page: Page) => void
  setSetting: <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => void
  hydratePersistedSettings: (
    settings: Pick<AppSettings, 'minimizeToTray' | 'gameUrl' | 'startupBrowserConcurrency'>,
  ) => void
  selectAccounts: (ids: string[]) => void
  setInventoryTab: (tab: UIState['inventoryTab']) => void
  setWhereToHuntAccountId: (accountId: string) => void
  setWhereHuntReference: (reference: HuntReference) => void
  openAccountDetail: (accountId: string) => void
  closeAccountDetail: () => void
  applyAutomation: (kind: AutomationKind, enabled: boolean) => void
  setPotionThreshold: (threshold: number) => void
  addMockAccounts: () => void
  setRealAccounts: (accounts: AccountView[]) => void
  setRealAccountDomain: <D extends AccountDomain>(
    domain: D,
    response: AccountDomainResponse<D>,
  ) => void
  advanceMock: () => void
}

const accountDomainKeys: AccountDomain[] = ['depot', 'inventory', 'hunt_options']
const domainViewKey: Record<AccountDomain, 'depot' | 'inventory' | 'huntOptions'> = {
  depot: 'depot',
  inventory: 'inventory',
  hunt_options: 'huntOptions',
}

const sameValue = (left: unknown, right: unknown): boolean => {
  if (Object.is(left, right)) return true
  if (!left || !right || typeof left !== 'object' || typeof right !== 'object') return false
  if (Array.isArray(left) || Array.isArray(right)) {
    return (
      Array.isArray(left) &&
      Array.isArray(right) &&
      left.length === right.length &&
      left.every((value, index) => sameValue(value, right[index]))
    )
  }
  const leftRecord = left as Record<string, unknown>
  const rightRecord = right as Record<string, unknown>
  const keys = Object.keys(leftRecord)
  return (
    keys.length === Object.keys(rightRecord).length &&
    keys.every((key) => sameValue(leftRecord[key], rightRecord[key]))
  )
}

const sameLiveSummary = (left: AccountView, right: AccountView) => {
  const heavyKeys = new Set(['depot', 'inventory', 'huntOptions', 'domainRevisions'])
  const leftRecord = left as unknown as Record<string, unknown>
  const rightRecord = right as unknown as Record<string, unknown>
  const keys = Object.keys(leftRecord).filter((key) => !heavyKeys.has(key))
  return (
    keys.length === Object.keys(rightRecord).filter((key) => !heavyKeys.has(key)).length &&
    keys.every((key) => sameValue(leftRecord[key], rightRecord[key]))
  )
}

const reconcileLiveAccount = (
  incoming: AccountView,
  previous?: AccountView,
  previousDomains?: AccountDomainCache,
): AccountView => {
  if (!previous || !incoming.domainRevisions || !previous.domainRevisions) return incoming
  const reconciled = { ...incoming }
  for (const domain of accountDomainKeys) {
    const sameRevision = incoming.domainRevisions[domain] === previous.domainRevisions[domain]
    // Keep the last usable snapshot visible while its newer revision loads.
    // The cache entry, not the incoming summary's empty placeholder, is the
    // source of truth for whether this domain has been loaded before.
    if (domain === 'depot') {
      const cachedData = previousDomains?.depot?.data
      if (cachedData !== undefined) reconciled.depot = cachedData
      else if (sameRevision) reconciled.depot = previous.depot
    } else if (domain === 'inventory') {
      const cachedData = previousDomains?.inventory?.data
      if (cachedData !== undefined) reconciled.inventory = cachedData
      else if (sameRevision) reconciled.inventory = previous.inventory
    } else {
      const cachedData = previousDomains?.hunt_options?.data
      if (cachedData !== undefined) reconciled.huntOptions = cachedData
      else if (sameRevision) reconciled.huntOptions = previous.huntOptions
    }
  }
  const domainsUnchanged = accountDomainKeys.every((domain) => {
    const viewKey = domainViewKey[domain]
    return previous[viewKey] === reconciled[viewKey]
  })
  const revisionsUnchanged = accountDomainKeys.every(
    (domain) => incoming.domainRevisions?.[domain] === previous.domainRevisions?.[domain],
  )
  return sameLiveSummary(previous, reconciled) && domainsUnchanged && revisionsUnchanged
    ? previous
    : reconciled
}

const sameDomainCaches = (
  previous: Record<string, AccountDomainCache> | undefined,
  next: Record<string, AccountDomainCache>,
) => {
  const previousIds = Object.keys(previous ?? {})
  const nextIds = Object.keys(next)
  return (
    previousIds.length === nextIds.length &&
    nextIds.every((id) => {
      const before = previous?.[id]
      const after = next[id]
      if (!before || !after) return false
      const beforeDomains = Object.keys(before) as AccountDomain[]
      const afterDomains = Object.keys(after) as AccountDomain[]
      return (
        beforeDomains.length === afterDomains.length &&
        afterDomains.every((domain) => before[domain] === after[domain])
      )
    })
  )
}

const initialSettings: AppSettings = {
  mockMode: false,
  developerMode: false,
  minimizeToTray: true,
  gameUrl: import.meta.env.VITE_GAME_URL ?? 'https://pokeidle.io/app',
  startupBrowserConcurrency: 1,
}
const cloneMockAccounts = () =>
  mockAccounts.map((account) => ({
    ...account,
    automations: { ...account.automations },
    drops: [...account.drops],
    types: [...account.types],
    inventory: account.inventory.map((item) => ({ ...item })),
    depot: account.depot.map((pokemon) => ({ ...pokemon, types: [...pokemon.types] })),
    depotLocks: { ...account.depotLocks },
    wildPokemon: account.wildPokemon ? { ...account.wildPokemon } : null,
  }))
export const selectVisibleAccounts = (state: AppState) =>
  state.settings.mockMode ? state.mock.accounts : state.real.accounts

export const useAppStore = create<AppState>((set) => ({
  real: { accounts: [], domains: {} },
  mock: { accounts: [], tick: 0 },
  whereHuntReference: null,
  ui: {
    page: 'Dashboard',
    selectedIds: [],
    inventoryTab: 'items',
    detailAccountId: null,
    whereToHuntAccountId: null,
  },
  settings: initialSettings,
  setPage: (page) => set((state) => ({ ui: { ...state.ui, page, detailAccountId: null } })),
  setSetting: (key, value) =>
    set((state) => {
      const settings = { ...state.settings, [key]: value }
      if (key === 'mockMode') {
        const enabled = value as AppSettings['mockMode']
        return {
          settings,
          mock: enabled ? { accounts: cloneMockAccounts(), tick: 0 } : { accounts: [], tick: 0 },
          ui: {
            ...state.ui,
            selectedIds: enabled ? mockAccounts.map((account) => account.id) : [],
            detailAccountId: null,
          },
        }
      }
      return { settings }
    }),
  hydratePersistedSettings: (persisted) =>
    set((state) => ({ settings: { ...state.settings, ...persisted } })),
  selectAccounts: (selectedIds) => set((state) => ({ ui: { ...state.ui, selectedIds } })),
  setInventoryTab: (inventoryTab) => set((state) => ({ ui: { ...state.ui, inventoryTab } })),
  setWhereToHuntAccountId: (whereToHuntAccountId) =>
    set((state) => ({ ui: { ...state.ui, whereToHuntAccountId } })),
  setWhereHuntReference: (whereHuntReference) => set({ whereHuntReference }),
  openAccountDetail: (detailAccountId) =>
    set((state) => ({ ui: { ...state.ui, detailAccountId } })),
  closeAccountDetail: () => set((state) => ({ ui: { ...state.ui, detailAccountId: null } })),
  applyAutomation: (kind, enabled) =>
    set((state) => {
      const update = (accounts: AccountView[]) =>
        accounts.map((account) => {
          if (!state.ui.selectedIds.includes(account.id)) return account
          const automations = { ...account.automations, [kind]: enabled }
          if (enabled && kind === 'ballUntilCapture') automations.ballContinuous = false
          if (enabled && kind === 'ballContinuous') automations.ballUntilCapture = false
          return { ...account, automations }
        })
      return state.settings.mockMode
        ? { mock: { ...state.mock, accounts: update(state.mock.accounts) } }
        : { real: { ...state.real, accounts: update(state.real.accounts) } }
    }),
  setPotionThreshold: (potionThreshold) =>
    set((state) => {
      const update = (accounts: AccountView[]) =>
        accounts.map((account) =>
          state.ui.selectedIds.includes(account.id) ? { ...account, potionThreshold } : account,
        )
      return state.settings.mockMode
        ? { mock: { ...state.mock, accounts: update(state.mock.accounts) } }
        : { real: { ...state.real, accounts: update(state.real.accounts) } }
    }),
  addMockAccounts: () =>
    set((state) => ({
      mock: { accounts: cloneMockAccounts(), tick: 0 },
      ui: { ...state.ui, selectedIds: mockAccounts.map((account) => account.id) },
    })),
  setRealAccounts: (incomingAccounts) =>
    set((state) => {
      const previousById = new Map(state.real.accounts.map((account) => [account.id, account]))
      const accounts = incomingAccounts.map((account) =>
        reconcileLiveAccount(
          account,
          previousById.get(account.id),
          state.real.domains?.[account.id],
        ),
      )
      const domains: Record<string, AccountDomainCache> = {}
      for (const account of accounts) {
        const revisions = account.domainRevisions
        const previousDomains = state.real.domains?.[account.id]
        if (!revisions || !previousDomains) continue
        const retained: AccountDomainCache = {}
        for (const domain of accountDomainKeys) {
          const entry = previousDomains[domain]
          if (entry) retained[domain] = entry as never
        }
        if (Object.keys(retained).length) domains[account.id] = retained
      }
      const selectedIds = state.ui.selectedIds.filter((id) =>
        accounts.some((account) => account.id === id),
      )
      const accountsUnchanged =
        accounts.length === state.real.accounts.length &&
        accounts.every((account, index) => account === state.real.accounts[index])
      const selectionUnchanged =
        selectedIds.length === state.ui.selectedIds.length &&
        selectedIds.every((id, index) => id === state.ui.selectedIds[index])
      const domainsUnchanged = sameDomainCaches(state.real.domains, domains)
      if (accountsUnchanged && selectionUnchanged && domainsUnchanged) return state
      return {
        real: { accounts, domains: domainsUnchanged ? state.real.domains : domains },
        ui: {
          ...state.ui,
          selectedIds: selectionUnchanged ? state.ui.selectedIds : selectedIds,
        },
      }
    }),
  setRealAccountDomain: (domain, response) =>
    set((state) => {
      const accountIndex = state.real.accounts.findIndex(
        (account) => account.id === response.accountId,
      )
      if (accountIndex < 0 || !response.changed || response.data == null) return {}
      const account = state.real.accounts[accountIndex]
      if (account.domainRevisions?.[domain] !== response.revision) return {}

      const viewKey = domainViewKey[domain]
      const updatedAccount = { ...account, [viewKey]: response.data } as AccountView
      const accounts = [...state.real.accounts]
      accounts[accountIndex] = updatedAccount
      const currentCache = state.real.domains?.[account.id] ?? {}
      const domains = {
        ...state.real.domains,
        [account.id]: {
          ...currentCache,
          [domain]: { revision: response.revision, data: response.data },
        },
      }
      return { real: { accounts, domains } }
    }),
  advanceMock: () =>
    set((state) => {
      if (!state.settings.mockMode) return {}
      const tick = state.mock.tick + 1
      return {
        mock: {
          tick,
          accounts: state.mock.accounts.map((account, index) => {
            const farming = account.status === 'online' && account.hunt !== null
            if (!farming) return { ...account, xpPerHour: 0, goldPerHour: 0 }
            const xpReward = 1800 + ((tick + index * 3) % 7) * 113
            const goldReward = 330 + ((tick + index) % 5) * 31
            const seconds = Math.max(60, account.onlineSeconds + tick * 2)
            const kills = account.kills + 1
            const usePotion =
              account.automations.autoPotion &&
              (account.hp / account.maxHp) * 100 <= account.potionThreshold &&
              account.potions > 0
            const useBall =
              account.automations.ballContinuous && account.balls > 0 && tick % 3 === 0
            return {
              ...account,
              onlineSeconds: account.onlineSeconds + 2,
              xp: account.xp + xpReward,
              gold: account.gold + goldReward,
              xpGained: account.xpGained + xpReward,
              goldGained: account.goldGained + goldReward,
              kills,
              xpPerHour: Math.round(((account.xpGained + xpReward) * 3600) / seconds),
              goldPerHour: Math.round(((account.goldGained + goldReward) * 3600) / seconds),
              hp: usePotion
                ? Math.min(account.maxHp, account.hp + Math.round(account.maxHp * 0.18))
                : Math.max(1, account.hp - Math.round(account.maxHp * 0.025)),
              potions: usePotion ? account.potions - 1 : account.potions,
              balls: useBall ? account.balls - 1 : account.balls,
              wildPokemon: account.wildPokemon
                ? { ...account.wildPokemon, hp: Math.max(0, account.wildPokemon.hp - 220) }
                : null,
            }
          }),
        },
      }
    }),
}))

export const visibleAccounts = () => selectVisibleAccounts(useAppStore.getState())
