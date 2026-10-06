import { afterEach } from 'vitest'
import { useAppStore } from './stores/app-store'

;(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true

afterEach(() =>
  useAppStore.setState({
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
    settings: {
      mockMode: false,
      developerMode: false,
      minimizeToTray: true,
      gameUrl: 'https://pokeidle.io/app',
      startupBrowserConcurrency: 1,
    },
  }),
)
