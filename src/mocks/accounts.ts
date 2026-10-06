import type { AccountView, AutomationKind, DepotLocks, DepotPokemon, InventoryItem } from '../types'

// Synthetic demo fixtures only. Never place real account snapshots in this file.
const automations = (): Record<AutomationKind, boolean> => ({
  autoPotion: true,
  autoRevive: true,
  ballUntilCapture: false,
  ballContinuous: false,
  autoVendaLoot: true,
  returnToHunt: true,
  autoBuyPotion: false,
  autoBuyBall: false,
})

const detail = (
  overrides: Partial<AccountView>,
): Pick<
  AccountView,
  | 'xp'
  | 'xpLevel'
  | 'xpNext'
  | 'power'
  | 'quality'
  | 'types'
  | 'vip'
  | 'map'
  | 'onlineSeconds'
  | 'wildPokemon'
  | 'kills'
  | 'captures'
  | 'xpGained'
  | 'goldGained'
  | 'drops'
  | 'potionsPerHour'
  | 'ballsPerHour'
  | 'revives'
  | 'otherItems'
> => ({
  xp: 0,
  xpLevel: 0,
  xpNext: 1,
  power: 0,
  quality: 1,
  types: [],
  vip: false,
  map: 'Kanto',
  onlineSeconds: 0,
  wildPokemon: null,
  kills: 0,
  captures: 0,
  xpGained: 0,
  goldGained: 0,
  drops: [],
  potionsPerHour: 0,
  ballsPerHour: 0,
  revives: 0,
  otherItems: 0,
  ...overrides,
})

const locks = (overrides: Partial<DepotLocks> = {}): DepotLocks => ({
  autoLockShiny: true,
  autoLockNota9: true,
  autoLockNotaMin: 3.5,
  autoLockP5: true,
  ...overrides,
})
const bag = (items: Array<[string, string, number, InventoryItem['category']]>): InventoryItem[] =>
  items.map(([id, name, quantity, category]) => ({ id, name, quantity, category }))
const depot = (items: DepotPokemon[]): DepotPokemon[] => items

export const mockAccounts: AccountView[] = [
  {
    id: 'mock-1',
    nick: 'DemoTrainerTwo',
    color: '#62d4b4',
    status: 'online',
    mode: 'background',
    level: 222,
    pokemon: 'Venusaur',
    hp: 4564,
    maxHp: 4740,
    xpPerHour: 123_860,
    goldPerHour: 22_180,
    gold: 78_038_504,
    gems: 0,
    diamonds: 0,
    potions: 256,
    balls: 928,
    hunt: 'Ancient Pupitar',
    automations: automations(),
    potionThreshold: 40,
    inventory: bag([
      ['204', 'Potion', 256, 'potions'],
      ['4', 'Ultra Ball', 928, 'balls'],
      ['stone-rock', 'Rock Stone', 40, 'stones'],
      ['stone-earth', 'Earth Stone', 1, 'stones'],
      ['token-outland', 'Outland Token', 12, 'other'],
    ]),
    depot: depot([
      { id: '3337383', name: 'Venusaur', level: 222, power: 762, quality: 1.408, note: 3.234, ivTotal: 149, types: ['GRASS', 'POISON'], locked: true, caughtAt: 1787994617521 },
      { id: '3338001', name: 'Ancient Pupitar', level: 150, power: 588, quality: 1.8, note: 3.016, ivTotal: 97, types: ['ROCK', 'GROUND'], locked: false, p5: true, caughtAt: 1787994817521 },
      { id: '3338002', name: 'Ivysaur', level: 118, power: 421, quality: 1.28, note: 2.9, ivTotal: 121, types: ['GRASS', 'POISON'], locked: false, caughtAt: 1787994717521 },
    ]),
    depotLocks: locks(),
    ...detail({
      xp: 178447515,
      xpLevel: 176000000,
      xpNext: 179916200,
      power: 762,
      quality: 1.408,
      types: ['GRASS', 'POISON'],
      vip: true,
      map: 'Outland',
      onlineSeconds: 7420,
      wildPokemon: { name: 'Ancient Pupitar', level: 150, hp: 6357, maxHp: 10080 },
      kills: 247,
      captures: 8,
      xpGained: 847000,
      goldGained: 153000,
      drops: ['Earth Ball ×4', 'Small Stone ×10'],
      potionsPerHour: 13,
      ballsPerHour: 5,
      revives: 8,
      otherItems: 132,
    }),
  },
  {
    id: 'mock-2',
    nick: 'DemoTrainerThree',
    color: '#8b9dff',
    status: 'online',
    mode: 'browser',
    level: 184,
    pokemon: 'Vaporeon',
    hp: 3210,
    maxHp: 3400,
    xpPerHour: 99_220,
    goldPerHour: 17_600,
    gold: 16_205_900,
    gems: 18,
    diamonds: 0,
    potions: 173,
    balls: 1_247,
    hunt: 'Seafoam Route',
    automations: automations(),
    potionThreshold: 45,
    inventory: bag([
      ['204', 'Potion', 173, 'potions'],
      ['4', 'Ultra Ball', 1247, 'balls'],
      ['stone-rock', 'Rock Stone', 20, 'stones'],
      ['stone-ice', 'Ice Stone', 1, 'stones'],
      ['revive', 'Revive', 4, 'other'],
    ]),
    depot: depot([
      { id: '4400101', name: 'Vaporeon', level: 184, power: 641, quality: 1.52, note: 3.55, ivTotal: 161, types: ['WATER'], locked: true, caughtAt: 1787014617521 },
      { id: '4400102', name: 'Lapras', level: 145, power: 553, quality: 1.61, note: 3.18, ivTotal: 145, types: ['WATER', 'ICE'], locked: false, shiny: true, caughtAt: 1787014717521 },
      { id: '4400103', name: 'Seel', level: 112, power: 344, quality: 1.19, note: 2.7, ivTotal: 113, types: ['WATER'], locked: false, caughtAt: 1787014817521 },
    ]),
    depotLocks: locks({ autoLockNota9: false }),
    ...detail({
      xp: 104502220,
      xpLevel: 103000000,
      xpNext: 105890000,
      power: 641,
      quality: 1.52,
      types: ['WATER'],
      onlineSeconds: 5210,
      wildPokemon: { name: 'Seel', level: 112, hp: 2280, maxHp: 3220 },
      kills: 188,
      captures: 12,
      xpGained: 620000,
      goldGained: 110000,
      drops: ['Water Ball ×2', 'Ice Stone ×1'],
      potionsPerHour: 8,
      ballsPerHour: 9,
      revives: 4,
      otherItems: 88,
    }),
  },
  {
    id: 'mock-3',
    nick: 'DemoTrainerFour',
    color: '#f2b86a',
    status: 'connecting',
    mode: 'transitioning',
    level: 96,
    pokemon: 'Charizard',
    hp: 0,
    maxHp: 2760,
    xpPerHour: 0,
    goldPerHour: 0,
    gold: 2_911_240,
    gems: 4,
    diamonds: 0,
    potions: 51,
    balls: 440,
    hunt: null,
    automations: automations(),
    potionThreshold: 35,
    inventory: bag([
      ['204', 'Potion', 51, 'potions'],
      ['4', 'Ultra Ball', 440, 'balls'],
      ['stone-rock', 'Rock Stone', 3, 'stones'],
      ['stone-fire', 'Fire Stone', 1, 'stones'],
      ['material-iron', 'Iron Fragment', 24, 'other'],
    ]),
    depot: depot([
      { id: '5100101', name: 'Charizard', level: 96, power: 404, quality: 1.22, note: 2.84, ivTotal: 105, types: ['FIRE', 'FLYING'], locked: true, caughtAt: 1786014617521 },
      { id: '5100102', name: 'Growlithe', level: 72, power: 288, quality: 1.15, note: 2.44, ivTotal: 96, types: ['FIRE'], locked: false, caughtAt: 1786014717521 },
    ]),
    depotLocks: locks({ autoLockP5: false }),
    ...detail({
      xp: 27900100,
      xpLevel: 27000000,
      xpNext: 28600000,
      power: 404,
      quality: 1.22,
      types: ['FIRE', 'FLYING'],
      revives: 2,
      otherItems: 40,
    }),
  },
  {
    id: 'mock-4',
    nick: 'DemoTrainerFive',
    color: '#e589b4',
    status: 'offline',
    mode: 'background',
    level: 71,
    pokemon: 'Kadabra',
    hp: 0,
    maxHp: 1990,
    xpPerHour: 0,
    goldPerHour: 0,
    gold: 820_100,
    gems: 0,
    diamonds: 0,
    potions: 0,
    balls: 0,
    hunt: null,
    automations: automations(),
    potionThreshold: 40,
    inventory: bag([
      ['stone-rock', 'Rock Stone', 0, 'stones'],
      ['stone-sun', 'Sun Stone', 1, 'stones'],
      ['token-kanto', 'Kanto Token', 7, 'other'],
    ]),
    depot: depot([
      { id: '6200101', name: 'Kadabra', level: 71, power: 280, quality: 1.1, note: 2.22, ivTotal: 89, types: ['PSYCHIC'], locked: false, caughtAt: 1785014617521 },
      { id: '6200102', name: 'Abra', level: 29, power: 104, quality: 1.05, note: 1.95, ivTotal: 75, types: ['PSYCHIC'], locked: false, caughtAt: 1785014717521 },
    ]),
    depotLocks: locks({ autoLockShiny: false, autoLockP5: false }),
    ...detail({
      xp: 11600200,
      xpLevel: 11000000,
      xpNext: 12100000,
      power: 280,
      quality: 1.1,
      types: ['PSYCHIC'],
      map: 'Saffron',
      otherItems: 19,
    }),
  },
]
