import { invoke } from '@tauri-apps/api/core'
import type { AccountDomain, AccountDomainResponse } from './types'
import { useAppStore } from './stores/app-store'

const commandFor: Record<AccountDomain, string> = {
  depot: 'account_depot',
  inventory: 'account_inventory',
  hunt_options: 'account_hunt_options',
}

const pendingRequests = new Map<string, Promise<void>>()

/** Fetches one heavy account domain only when its live revision is not cached. */
export function ensureAccountDomain<D extends AccountDomain>(accountId: string, domain: D) {
  const account = useAppStore.getState().real.accounts.find((entry) => entry.id === accountId)
  const revision = account?.domainRevisions?.[domain]
  if (revision === undefined) return Promise.resolve()

  const state = useAppStore.getState()
  const cached = state.real.domains?.[accountId]?.[domain]
  if (cached?.revision === revision) return Promise.resolve()

  const key = `${accountId}:${domain}:${revision}`
  const existing = pendingRequests.get(key)
  if (existing) return existing

  const request = invoke<AccountDomainResponse<D>>(commandFor[domain], {
    accountId,
    knownRevision: cached?.revision,
  })
    .then((response) => {
      useAppStore.getState().setRealAccountDomain(domain, response)
    })
    .finally(() => {
      pendingRequests.delete(key)
    })
  pendingRequests.set(key, request)
  return request
}
