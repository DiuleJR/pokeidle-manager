import type { AccountView, DepotPokemon, InventoryCategory, InventoryItem } from '../types'

export type OwnedItem = InventoryItem & {
  owners: Array<{ accountId: string; nick: string; quantity: number }>
}
export type OwnedPokemon = DepotPokemon & { accountId: string; nick: string; color: string }

export function inventoryAccounts(accounts: AccountView[], accountId: string) {
  return accountId === 'all' ? accounts : accounts.filter((account) => account.id === accountId)
}
export function aggregateItems(accounts: AccountView[], accountId = 'all'): OwnedItem[] {
  const grouped = new Map<string, OwnedItem>()
  for (const account of inventoryAccounts(accounts, accountId)) {
    for (const item of account.inventory) {
      if (!item.quantity) continue
      const current = grouped.get(item.id)
      if (current) {
        current.quantity += item.quantity
        current.owners.push({ accountId: account.id, nick: account.nick, quantity: item.quantity })
      } else {
        grouped.set(item.id, { ...item, owners: [{ accountId: account.id, nick: account.nick, quantity: item.quantity }] })
      }
    }
  }
  return [...grouped.values()]
}
export function depotPokemon(accounts: AccountView[], accountId = 'all'): OwnedPokemon[] {
  return inventoryAccounts(accounts, accountId).flatMap((account) =>
    account.depot.map((pokemon) => ({ ...pokemon, accountId: account.id, nick: account.nick, color: account.color })),
  )
}
export function filterItems(items: OwnedItem[], query: string, category: InventoryCategory | 'all') {
  const normalized = query.trim().toLowerCase()
  return items.filter((item) =>
    (category === 'all' || item.category === category) && (!normalized || item.name.toLowerCase().includes(normalized)),
  )
}
export type PokemonSort = 'level' | 'quality' | 'power' | 'note' | 'iv' | 'type' | 'recent'
export function filterPokemon(pokemon: OwnedPokemon[], query: string, type: string, minIv: number | null, order: PokemonSort) {
  const normalized = query.trim().toLowerCase()
  return pokemon
    .filter((entry) =>
      (!normalized || entry.name.toLowerCase().includes(normalized)) &&
      (type === 'all' || entry.types.includes(type)) &&
      (minIv === null || (entry.ivTotal ?? 0) >= minIv),
    )
    .sort((a, b) => {
      if (order === 'type') return a.types.join('/').localeCompare(b.types.join('/'))
      if (order === 'recent') return (b.caughtAt ?? 0) - (a.caughtAt ?? 0)
      const field = order === 'iv' ? 'ivTotal' : order
      return (b[field] ?? 0) - (a[field] ?? 0)
    })
}
