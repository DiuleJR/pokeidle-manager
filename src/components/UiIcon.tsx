import type { ImgHTMLAttributes } from 'react'

import account from '../assets/ui-icons/account.svg'
import accounts from '../assets/ui-icons/accounts.svg'
import attack from '../assets/ui-icons/attack.svg'
import automation from '../assets/ui-icons/automation.svg'
import background from '../assets/ui-icons/background.svg'
import broadcast from '../assets/ui-icons/broadcast.svg'
import buyBall from '../assets/ui-icons/buy-ball.svg'
import buyPotion from '../assets/ui-icons/buy-potion.svg'
import capture from '../assets/ui-icons/capture.svg'
import close from '../assets/ui-icons/close.svg'
import dashboard from '../assets/ui-icons/dashboard.svg'
import defense from '../assets/ui-icons/defense.svg'
import diamond from '../assets/ui-icons/diamond.svg'
import drop from '../assets/ui-icons/drop.svg'
import event from '../assets/ui-icons/event.svg'
import farm from '../assets/ui-icons/farm.svg'
import gem from '../assets/ui-icons/gem.svg'
import gold from '../assets/ui-icons/gold.svg'
import guild from '../assets/ui-icons/guild.svg'
import guildBoost from '../assets/ui-icons/guild-boost.svg'
import hp from '../assets/ui-icons/hp.svg'
import hunt from '../assets/ui-icons/hunt.svg'
import inventory from '../assets/ui-icons/inventory.svg'
import level from '../assets/ui-icons/level.svg'
import map from '../assets/ui-icons/map.svg'
import market from '../assets/ui-icons/market.svg'
import maximize from '../assets/ui-icons/maximize.svg'
import metrics from '../assets/ui-icons/metrics.svg'
import minimize from '../assets/ui-icons/minimize.svg'
import pokemon from '../assets/ui-icons/pokemon.svg'
import potion from '../assets/ui-icons/potion.svg'
import revive from '../assets/ui-icons/revive.svg'
import repeat from '../assets/ui-icons/repeat.svg'
import sale from '../assets/ui-icons/sale.svg'
import settings from '../assets/ui-icons/settings.svg'
import shield from '../assets/ui-icons/shield.svg'
import specialAttack from '../assets/ui-icons/special-attack.svg'
import specialDefense from '../assets/ui-icons/special-defense.svg'
import speed from '../assets/ui-icons/speed.svg'
import xp from '../assets/ui-icons/xp.svg'
import vip from '../assets/ui-icons/vip.svg'
import browser from '../assets/ui-icons/browser.svg'
import clock from '../assets/ui-icons/clock.svg'
import returnIcon from '../assets/ui-icons/return.svg'

const iconAssets = {
  account,
  accounts,
  attack,
  automation,
  background,
  broadcast,
  browser,
  buyBall,
  buyPotion,
  capture,
  clock,
  close,
  dashboard,
  defense,
  diamond,
  drop,
  event,
  farm,
  gem,
  gold,
  guild,
  guildBoost,
  hp,
  hunt,
  inventory,
  level,
  map,
  market,
  maximize,
  metrics,
  minimize,
  pokemon,
  potion,
  revive,
  repeat,
  return: returnIcon,
  sale,
  settings,
  shield,
  specialAttack,
  specialDefense,
  speed,
  vip,
  xp,
} as const

export type UiIconName = keyof typeof iconAssets

type UiIconProps = Omit<ImgHTMLAttributes<HTMLImageElement>, 'src' | 'alt'> & {
  name: UiIconName
}

export function UiIcon({ name, className, ...props }: UiIconProps) {
  return (
    <img
      {...props}
      className={['ui-icon', className].filter(Boolean).join(' ')}
      src={iconAssets[name]}
      alt=""
      aria-hidden="true"
      draggable={false}
    />
  )
}
