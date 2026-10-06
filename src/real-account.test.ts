import { describe, expect, it } from 'vitest'
import { accountFromRuntime, formatHuntSlug } from './real-account'

describe('real account presentation', () => {
  it('formats a hunt slug only as a display fallback', () => {
    expect(formatHuntSlug('ancient_pupitar')).toBe('Ancient Pupitar')
    expect(formatHuntSlug('seafoam_route')).toBe('Seafoam Route')
  })
  it('passes the persisted hunt start timestamp to the account card model', () => {
    const account = accountFromRuntime({
      account: { id: 'a', nick: 'ash', card_color: '#fff', status: 'online', mode: 'background' },
      state: {
        hunt_slug: 'ancient_pupitar',
        hunt_started_at_ms: 1_700_000_000_000,
        no_centro: false,
        items: {}, balls: {}, pokemon: [],
        automation: { potion_ids: [], ball_ids: [], revive_ids: [] },
      },
      metrics: { xp_per_hour: 0, gold_per_hour: 0, kills: 0, captures: 0 },
    } as never)
    expect(account.huntStartedAtMs).toBe(1_700_000_000_000)
  })
  it('uses the latest server timestamp to present the VIP state', () => {
    const base = {
      account: { id: 'a', nick: 'ash', card_color: '#fff', status: 'online', mode: 'background' },
      state: {
        vip_until: 2_000,
        server_now: 1_999,
        no_centro: false,
        items: {}, balls: {}, pokemon: [],
        automation: { potion_ids: [], ball_ids: [], revive_ids: [] },
      },
      metrics: { xp_per_hour: 0, gold_per_hour: 0, kills: 0, captures: 0 },
    }
    expect(accountFromRuntime(base as never).vip).toBe(true)
    expect(accountFromRuntime({ ...base, state: { ...base.state, server_now: 2_000 } } as never).vip).toBe(false)
  })
  it('does not report a missing store VIP schema as inactive', () => {
    const account = accountFromRuntime({
      account: { id: 'a', nick: 'ash', card_color: '#fff', status: 'online', mode: 'background' },
      state: {
        vip_data_available: true,
        no_centro: false,
        items: {}, balls: {}, pokemon: [],
        automation: { potion_ids: [], ball_ids: [], revive_ids: [] },
      },
      metrics: { xp_per_hour: 0, gold_per_hour: 0, kills: 0, captures: 0 },
    } as never)
    expect(account.vip).toBeUndefined()
    expect(account.vipDataAvailable).toBe(true)
  })
  it('prefers the confirmed VIP values inside estado.loja', () => {
    const account = accountFromRuntime({
      account: { id: 'a', nick: 'ash', card_color: '#fff', status: 'online', mode: 'background' },
      state: {
        vip_data_available: true,
        vip_active: false,
        vip_until: 0,
        no_centro: false,
        items: {}, balls: {}, pokemon: [],
        automation: { potion_ids: [], ball_ids: [], revive_ids: [] },
      },
      metrics: { xp_per_hour: 0, gold_per_hour: 0, kills: 0, captures: 0 },
    } as never)
    expect(account.vip).toBe(false)
    expect(account.vipExpiresAt).toBe(0)
  })
  it('derives the active Pokémon from activeId after a partial pkMud update', () => {
    const account = accountFromRuntime({
      account: { id: 'a', nick: 'ash', card_color: '#fff', status: 'online', mode: 'background' },
      state: {
        active_id: 2968063,
        no_centro: false,
        items: {}, balls: {},
        pokemon: [
          { id: 2968063, nome: 'Venusaur', level: 486, hp: 7040, maxHp: 9924, xp: 1890014840, xp_level: 1889705500, xp_next: 1901442600 },
          { id: 3289961, nome: 'Tyranitar', level: 300, hp: 3936, maxHp: 3936 },
        ],
        automation: { potion_ids: [], ball_ids: [], revive_ids: [] },
      },
      metrics: { xp_per_hour: 0, gold_per_hour: 0, kills: 0, captures: 0 },
    } as never)
    expect(account.pokemon).toBe('Venusaur')
    expect(account.hp).toBe(7040)
    expect(account.maxHp).toBe(9924)
    expect(account.xp).toBe(1890014840)
  })
  it('exposes the active Pokémon IVs, hunt composition, and XP modifiers for estimation', () => {
    const account = accountFromRuntime({
      account: { id: 'a', nick: 'ash', card_color: '#fff', status: 'online', mode: 'background' },
      state: {
        level: 1153,
        active_id: 7,
        no_centro: false,
        xp_bonus: { eventTrainerPct: 10, eventPokemonPct: 10, guildRankPct: 5, guildBoostActive: true, twitchPct: 22.5 },
        hunts: [{ slug: 'shedinja', name: 'Shedinja', level: 500, species: [{ species_id: 292, weight: 15 }] }],
        items: {}, balls: {},
        pokemon: [{
          id: 7, nome: 'Tyranitar', level: 722, hp: 100, maxHp: 200,
          quality: 1.711, potencia: 1, shiny: false, species_id: 248,
          ivs: { hp: 17, atk: 23, def: 20, spAtk: 28, spDef: 32, speed: 32 },
        }],
        automation: { potion_ids: [], ball_ids: [], revive_ids: [] },
      },
      metrics: { xp_per_hour: 0, gold_per_hour: 0, kills: 0, captures: 0 },
    } as never)

    expect(account.trainerLevel).toBe(1153)
    expect(account.activePokemon?.speciesId).toBe(248)
    expect(account.activePokemon?.ivs?.speed).toBe(32)
    expect(account.huntOptions?.[0].species?.[0]).toEqual({ speciesId: 292, weight: 15 })
    expect(account.xpBonus?.guildRankPct).toBe(5)
    expect(account.xpBonus?.guildBoostActive).toBe(true)
    expect(account.xpBonus?.twitchPct).toBe(22.5)
    expect(account.xpBonus?.eventTrainerPct).toBe(10)
    expect(account.xpBonus?.eventPokemonPct).toBe(10)
  })
  it('does not treat the first configured Potion as the active Potion', () => {
    const base = {
      account: { id: 'a', nick: 'ash', card_color: '#fff', status: 'online', mode: 'background' },
      state: {
        no_centro: false,
        active_potion_id: null,
        items: { '204': 10, '202': 3 }, balls: {}, pokemon: [],
        automation: { potion_ids: [204, 202], ball_ids: [], revive_ids: [] },
      },
      metrics: { xp_per_hour: 0, gold_per_hour: 0, kills: 0, captures: 0 },
    }
    expect(accountFromRuntime(base as never).potionName).toBe('Ultimate Potion')
    const observed = accountFromRuntime({
      ...base,
      state: { ...base.state, active_potion_id: 202, active_potion_source: 'inventory_delta' },
      metrics: { ...base.metrics, potions_per_hour: 12 },
    } as never)
    expect(observed.potionName).toBe('Ultra Potion')
    expect(observed.potions).toBe(3)
    expect(observed.potionsPerHour).toBe(12)
  })
})
