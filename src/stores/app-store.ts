import { create } from 'zustand'
import type { HuntReference } from '../hunts/calculator'
import { mockAccounts } from '../mocks/accounts'
import type { AccountView, AppSettings, AutomationKind, Page } from '../types'

export interface RealAccountState {
  accounts: AccountView[]
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
  advanceMock: () => void
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
  real: { accounts: [] },
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
        : { real: { accounts: update(state.real.accounts) } }
    }),
  setPotionThreshold: (potionThreshold) =>
    set((state) => {
      const update = (accounts: AccountView[]) =>
        accounts.map((account) =>
          state.ui.selectedIds.includes(account.id) ? { ...account, potionThreshold } : account,
        )
      return state.settings.mockMode
        ? { mock: { ...state.mock, accounts: update(state.mock.accounts) } }
        : { real: { accounts: update(state.real.accounts) } }
    }),
  addMockAccounts: () =>
    set((state) => ({
      mock: { accounts: cloneMockAccounts(), tick: 0 },
      ui: { ...state.ui, selectedIds: mockAccounts.map((account) => account.id) },
    })),
  setRealAccounts: (accounts) =>
    set((state) => ({
      real: { accounts },
      ui: {
        ...state.ui,
        selectedIds: state.ui.selectedIds.filter((id) => accounts.some((account) => account.id === id)),
      },
    })),
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
            const useBall = account.automations.ballContinuous && account.balls > 0 && tick % 3 === 0
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
