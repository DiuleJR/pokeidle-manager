import type { MobileInventoryPage } from './contract'

const ttlMs = 30_000
const maxEntries = 12
const pages = new Map<string, { page: MobileInventoryPage; storedAt: number }>()

export function getMobileInventoryPage(key: string): MobileInventoryPage | null {
  const cached = pages.get(key)
  if (!cached) return null
  if (Date.now() - cached.storedAt >= ttlMs) {
    pages.delete(key)
    return null
  }
  return cached.page
}

export function cacheMobileInventoryPage(key: string, page: MobileInventoryPage) {
  pages.delete(key)
  pages.set(key, { page, storedAt: Date.now() })
  while (pages.size > maxEntries) {
    const oldest = pages.keys().next().value as string | undefined
    if (!oldest) break
    pages.delete(oldest)
  }
}

export function invalidateMobileInventoryPage(key: string) {
  pages.delete(key)
}

export function clearMobileInventoryCacheForTests() {
  pages.clear()
}
