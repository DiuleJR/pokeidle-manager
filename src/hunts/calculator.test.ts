import { describe, expect, it } from 'vitest'
import type { AccountView } from '../types'
import { bonusMultiplier, estimateHunts, readHuntReference, type HuntReference } from './calculator'

const reference: HuntReference = {
  looktypes: { '10': 60292 },
  tipos: { NORMAL: { ROCK: 0.5 }, WATER: { ROCK: 2 } },
  especies: [
    { id: 1, n: 'Tyranitar', t: ['ROCK'], b: [100, 134, 110, 95, 100, 61], m: [['Rock Slide', 'ROCK', 0, 75, 20_000, 1]] },
    { id: 10, n: 'Shedinja', t: ['BUG', 'GHOST'], b: [1, 90, 45, 30, 30, 40], m: [], l: [['Bug Antenna', 40, 1, 2, 96]] },
  ],
  hunts: [{ s: 'shedinja', n: 'Shedinja', a: 'kanto', nv: 10, e: [[10, 15]] }],
}

const account = (overrides: Partial<AccountView> = {}) => ({
  id: 'account-1', nick: 'Test', color: '#fff', status: 'online', mode: 'background',
  level: 100, trainerLevel: 100, xp: 0, xpLevel: 0, xpNext: 0, power: 1, quality: 1,
  types: ['ROCK'], map: 'Kanto', onlineSeconds: 0, pokemon: 'Tyranitar', hp: 1000,
  maxHp: 1000, xpPerHour: 0, goldPerHour: 0, gold: 0, gems: 0, diamonds: 0,
  potions: 0, balls: 0, hunt: null, wildPokemon: null, kills: 0, captures: 0,
  xpGained: 0, goldGained: 0, drops: [], potionsPerHour: 0, ballsPerHour: 0,
  revives: 0, otherItems: 0, automations: {}, potionThreshold: 50, inventory: [], depot: [],
  depotLocks: { autoLockShiny: false, autoLockNota9: false, autoLockNotaMin: 0, autoLockP5: false },
  activePokemon: {
    id: '7', name: 'Tyranitar', level: 100, speciesId: 1, quality: 1, potency: 1,
    types: ['ROCK'], ivs: { hp: 16, atk: 16, def: 16, spAtk: 16, spDef: 16, speed: 16 },
  },
  ...overrides,
}) as AccountView

describe('where-to-hunt estimates', () => {
  it('multiplies independently active XP modifiers', () => {
    const withBonuses = account({
      vip: true,
      xpBonus: { guildRankPct: 5, guildBoostActive: true, twitchPct: 25, eventTrainerPct: 10 },
    })
    expect(bonusMultiplier(withBonuses, true)).toBeCloseTo(1.5 * 1.05 * 1.1 * 1.25 * 1.1)
    expect(bonusMultiplier(withBonuses, false)).toBeCloseTo(1.5 * 1.05 * 1.1 * 1.25)
  })

  it('ranks level-eligible hunts from the active Pokémon and wild composition', () => {
    const results = estimateHunts(account(), readHuntReference(reference)!)
    expect(results).toHaveLength(1)
    expect(results[0].hunt.s).toBe('shedinja')
    expect(results[0].primarySpecies.id).toBe(10)
    expect(results[0].primarySpecies.looktype).toBe(60292)
    expect(results[0].secondsPerKill).toBeGreaterThan(2.8)
    expect(results[0].xpTrainerPerHour).toBeGreaterThan(0)
    expect(results[0].goldPerHour).toBeGreaterThan(0)
  })

  it('does not calculate when individual IV data is incomplete', () => {
    const incomplete = account({
      activePokemon: {
        id: '7', name: 'Tyranitar', level: 100, speciesId: 1, quality: 1, potency: 1,
        types: ['ROCK'], ivs: { hp: 16, atk: 16 },
      },
    })
    expect(estimateHunts(incomplete, reference)).toEqual([])
  })

  it('attaches only positive integer looktypes from the game catalog to species', () => {
    const parsed = readHuntReference({
      ...reference,
      looktypes: { '10': 60292, '1': 1, invalid: 60293 },
    })
    expect(parsed?.especies.find((species) => species.id === 10)?.looktype).toBe(60292)
    expect(parsed?.especies.find((species) => species.id === 1)?.looktype).toBeUndefined()
    expect(parsed?.looktypes).toEqual({ '10': 60292 })
  })

  it('uses the base species sprite when the guide species is a Mega form', () => {
    const parsed = readHuntReference({
      ...reference,
      especies: [...reference.especies, { ...reference.especies[0], id: 3001, n: 'Mega Tyranitar' }],
      looktypes: { '1': 60001 },
    })
    expect(parsed?.especies.find((species) => species.id === 3001)?.looktype).toBe(60001)
  })
})
