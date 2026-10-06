import { describe, expect, it } from 'vitest'
import { mockAccounts } from '../mocks/accounts'
import { aggregateItems, depotPokemon, filterItems, filterPokemon } from './selectors'

describe('inventory selectors', () => {
  it('aggregates identical items when all accounts are selected', () => {
    const rockStone = aggregateItems(mockAccounts).find((item) => item.id === 'stone-rock')
    expect(rockStone?.quantity).toBe(63)
    expect(rockStone?.owners).toHaveLength(3)
  })
  it('returns only the selected account items', () => {
    const items = aggregateItems(mockAccounts, 'mock-2')
    expect(items.find((item) => item.name === 'Rock Stone')?.quantity).toBe(20)
    expect(items.some((item) => item.name === 'Earth Stone')).toBe(false)
  })
  it('filters items by search and category', () => {
    expect(filterItems(aggregateItems(mockAccounts), 'rock', 'stones').map((item) => item.name)).toEqual(['Rock Stone'])
  })
  it('updates the depot when a single account is selected', () => {
    const entries = depotPokemon(mockAccounts, 'mock-3')
    expect(entries).toHaveLength(2)
    expect(entries.every((entry) => entry.accountId === 'mock-3')).toBe(true)
  })
  it('keeps the owner alongside every pokemon in the all-accounts view', () => {
    const lapras = depotPokemon(mockAccounts).find((entry) => entry.name === 'Lapras')
    expect(lapras).toMatchObject({ accountId: 'mock-2', nick: 'DemoTrainerThree' })
  })
  it('filters Pokémon using documented type and IV data', () => {
    const entries = filterPokemon(depotPokemon(mockAccounts), '', 'WATER', 150, 'level')
    expect(entries.map((entry) => entry.name)).toEqual(['Vaporeon'])
  })
  it('orders Pokémon by the selected supported metric', () => {
    const entries = filterPokemon(depotPokemon(mockAccounts), '', 'all', null, 'quality')
    expect(entries[0].name).toBe('Ancient Pupitar')
  })
})
