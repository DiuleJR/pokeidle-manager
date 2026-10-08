import { beforeEach, describe, expect, it } from 'vitest'
import { mockAccounts } from '../mocks/accounts'
import type { AccountView, DepotPokemon, InventoryItem } from '../types'
import { useAppStore } from './app-store'

const makeLiveAccount = (
  domainRevisions = { depot: 1, inventory: 1, hunt_options: 1 },
): AccountView => ({
  ...mockAccounts[0],
  domainRevisions,
  depot: [],
  inventory: [],
  huntOptions: [],
})
describe('real account domain cache', () => {
  beforeEach(() => {
    useAppStore.setState((state) => ({
      ...state,
      real: { accounts: [], domains: {} },
      ui: { ...state.ui, selectedIds: [] },
    }))
  })

  it('does not publish a new store state for an identical live summary poll', () => {
    const account = makeLiveAccount()
    useAppStore.getState().setRealAccounts([account])
    const previous = useAppStore.getState()

    useAppStore.getState().setRealAccounts([makeLiveAccount()])

    expect(useAppStore.getState()).toBe(previous)
  })

  it('preserves loaded domain references across unchanged summary revisions', () => {
    const account = makeLiveAccount()
    useAppStore.getState().setRealAccounts([account])
    const depot: DepotPokemon[] = [
      {
        id: 'poke-1',
        name: 'Venusaur',
        level: 700,
        types: ['GRASS'],
        locked: false,
      },
    ]
    const inventory: InventoryItem[] = [
      {
        id: '204',
        assetKey: '204',
        name: 'Ultimate Potion',
        quantity: 7,
        category: 'potions',
      },
    ]
    const hunts = [
      {
        slug: 'ancient_pupitar',
        name: 'Ancient Pupitar',
        species: [{ speciesId: 1, weight: 100 }],
      },
    ]
    const store = useAppStore.getState()
    store.setRealAccountDomain('depot', {
      accountId: account.id,
      revision: 1,
      changed: true,
      data: depot,
    })
    store.setRealAccountDomain('inventory', {
      accountId: account.id,
      revision: 1,
      changed: true,
      data: inventory,
    })
    store.setRealAccountDomain('hunt_options', {
      accountId: account.id,
      revision: 1,
      changed: true,
      data: hunts,
    })

    const loaded = useAppStore.getState().real.accounts[0]
    const references = [loaded.depot, loaded.inventory, loaded.huntOptions]
    useAppStore.getState().setRealAccounts([makeLiveAccount()])

    const refreshed = useAppStore.getState().real.accounts[0]
    expect(refreshed).toBe(loaded)
    expect(refreshed.depot).toBe(references[0])
    expect(refreshed.inventory).toBe(references[1])
    expect(refreshed.huntOptions).toBe(references[2])
    expect(useAppStore.getState().real.domains?.[account.id].depot?.data).toBe(depot)
  })

  it('invalidates only the domain whose revision changed', () => {
    const account = makeLiveAccount()
    const depot: DepotPokemon[] = [
      {
        id: 'poke-1',
        name: 'Venusaur',
        level: 700,
        types: ['GRASS'],
        locked: false,
      },
    ]
    const inventory: InventoryItem[] = [
      {
        id: '204',
        assetKey: '204',
        name: 'Ultimate Potion',
        quantity: 7,
        category: 'potions',
      },
    ]
    useAppStore.getState().setRealAccounts([account])
    useAppStore.getState().setRealAccountDomain('depot', {
      accountId: account.id,
      revision: 1,
      changed: true,
      data: depot,
    })
    useAppStore.getState().setRealAccountDomain('inventory', {
      accountId: account.id,
      revision: 1,
      changed: true,
      data: inventory,
    })

    useAppStore
      .getState()
      .setRealAccounts([makeLiveAccount({ depot: 1, inventory: 2, hunt_options: 1 })])
    const refreshed = useAppStore.getState().real.accounts[0]

    expect(refreshed.depot).toBe(depot)
    expect(refreshed.inventory).toEqual([])
    expect(useAppStore.getState().real.domains?.[account.id].depot?.revision).toBe(1)
    expect(useAppStore.getState().real.domains?.[account.id].inventory).toBeUndefined()
  })

  it('ignores stale or account-mismatched domain responses', () => {
    const account = makeLiveAccount({ depot: 3, inventory: 1, hunt_options: 1 })
    useAppStore.getState().setRealAccounts([account])
    const stale: DepotPokemon[] = [
      {
        id: 'old',
        name: 'Oldmon',
        level: 10,
        types: [],
        locked: false,
      },
    ]

    useAppStore.getState().setRealAccountDomain('depot', {
      accountId: account.id,
      revision: 2,
      changed: true,
      data: stale,
    })
    useAppStore.getState().setRealAccountDomain('depot', {
      accountId: 'removed-account',
      revision: 3,
      changed: true,
      data: stale,
    })

    expect(useAppStore.getState().real.accounts[0].depot).toEqual([])
    expect(useAppStore.getState().real.domains?.[account.id]).toBeUndefined()
  })

  it('keeps the existing cache when a matching response reports no change', () => {
    const account = makeLiveAccount()
    const inventory: InventoryItem[] = [
      {
        id: '204',
        assetKey: '204',
        name: 'Ultimate Potion',
        quantity: 7,
        category: 'potions',
      },
    ]
    useAppStore.getState().setRealAccounts([account])
    useAppStore.getState().setRealAccountDomain('inventory', {
      accountId: account.id,
      revision: 1,
      changed: true,
      data: inventory,
    })

    useAppStore.getState().setRealAccountDomain('inventory', {
      accountId: account.id,
      revision: 1,
      changed: false,
    })

    expect(useAppStore.getState().real.accounts[0].inventory).toBe(inventory)
  })
})
