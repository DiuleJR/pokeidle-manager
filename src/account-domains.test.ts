import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mockAccounts } from './mocks/accounts'
import { useAppStore } from './stores/app-store'
import type { AccountView } from './types'

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))

const liveAccount = (revision = 1): AccountView => ({
  ...mockAccounts[0],
  id: `domain-account-${revision}`,
  domainRevisions: { depot: revision, inventory: revision, hunt_options: revision },
  depot: [],
  inventory: [],
  huntOptions: [],
})

describe('account domain requests', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    useAppStore.setState((state) => ({
      ...state,
      real: { accounts: [], domains: {} },
      ui: { ...state.ui, selectedIds: [] },
    }))
  })

  it('loads a domain once per account revision and reuses its cache', async () => {
    const account = liveAccount()
    useAppStore.getState().setRealAccounts([account])
    invokeMock.mockResolvedValue({
      accountId: account.id,
      revision: 1,
      changed: true,
      data: [{ id: '200', assetKey: '200', name: 'Poção', quantity: 4, category: 'potions' }],
    })
    const { ensureAccountDomain } = await import('./account-domains')

    await ensureAccountDomain(account.id, 'inventory')
    await ensureAccountDomain(account.id, 'inventory')

    expect(invokeMock).toHaveBeenCalledTimes(1)
    expect(invokeMock).toHaveBeenCalledWith('account_inventory', {
      accountId: account.id,
      knownRevision: undefined,
    })
    expect(useAppStore.getState().real.domains?.[account.id]?.inventory?.revision).toBe(1)
  })

  it('requests the domain again only when the live revision changes', async () => {
    const account = liveAccount()
    useAppStore.getState().setRealAccounts([account])
    invokeMock.mockResolvedValue({
      accountId: account.id,
      revision: 1,
      changed: true,
      data: [],
    })
    const { ensureAccountDomain } = await import('./account-domains')
    await ensureAccountDomain(account.id, 'depot')

    const changedAccount = {
      ...account,
      domainRevisions: { depot: 2, inventory: 1, hunt_options: 1 },
    }
    useAppStore.getState().setRealAccounts([changedAccount])
    invokeMock.mockResolvedValue({
      accountId: account.id,
      revision: 2,
      changed: true,
      data: [],
    })
    await ensureAccountDomain(account.id, 'depot')

    expect(invokeMock).toHaveBeenCalledTimes(2)
    expect(invokeMock).toHaveBeenLastCalledWith('account_depot', {
      accountId: account.id,
      knownRevision: undefined,
    })
  })

  it('deduplicates concurrent reads and discards an old revision response', async () => {
    const account = liveAccount()
    useAppStore.getState().setRealAccounts([account])
    let resolveRead:
      | ((response: {
          accountId: string
          revision: number
          changed: boolean
          data: Array<{ id: string; name: string; level: number; types: string[]; locked: boolean }>
        }) => void)
      | undefined
    invokeMock.mockReturnValue(
      new Promise((resolve) => {
        resolveRead = resolve
      }),
    )
    const { ensureAccountDomain } = await import('./account-domains')

    const first = ensureAccountDomain(account.id, 'depot')
    const duplicate = ensureAccountDomain(account.id, 'depot')
    expect(invokeMock).toHaveBeenCalledTimes(1)
    useAppStore.getState().setRealAccounts([
      {
        ...account,
        domainRevisions: { depot: 2, inventory: 1, hunt_options: 1 },
      },
    ])
    expect(useAppStore.getState().real.accounts[0].domainRevisions?.depot).toBe(2)
    resolveRead?.({
      accountId: account.id,
      revision: 1,
      changed: true,
      data: [{ id: 'stale', name: 'Stale', level: 1, types: [], locked: false }],
    })
    await Promise.all([first, duplicate])

    expect(useAppStore.getState().real.accounts[0].depot).toEqual([])
    await ensureAccountDomain(account.id, 'depot')
    expect(invokeMock).toHaveBeenCalledTimes(2)
  })

  it('allows a failed domain read to be retried at the same revision', async () => {
    const account = liveAccount()
    useAppStore.getState().setRealAccounts([account])
    invokeMock
      .mockRejectedValueOnce(new Error('temporary failure'))
      .mockResolvedValueOnce({
        accountId: account.id,
        revision: 1,
        changed: true,
        data: [],
      })
    const { ensureAccountDomain } = await import('./account-domains')

    await expect(ensureAccountDomain(account.id, 'inventory')).rejects.toThrow('temporary failure')
    await ensureAccountDomain(account.id, 'inventory')

    expect(invokeMock).toHaveBeenCalledTimes(2)
    expect(useAppStore.getState().real.domains?.[account.id]?.inventory?.revision).toBe(1)
  })
})
