const POKEMON_CARD_MIN_WIDTH = 175
const POKEMON_GRID_GAP = 10
const POKEMON_GRID_PADDING = 28
export const POKEMON_GRID_ROW_HEIGHT = 258
const POKEMON_GRID_OVERSCAN_ROWS = 2

export type VirtualGridWindow = {
  columns: number
  startIndex: number
  endIndex: number
  top: number
  totalHeight: number
}

export function pokemonGridWindow(
  itemCount: number,
  viewportWidth: number,
  viewportHeight: number,
  scrollTop: number,
): VirtualGridWindow {
  const contentWidth = Math.max(POKEMON_CARD_MIN_WIDTH, viewportWidth - POKEMON_GRID_PADDING)
  const columns = Math.max(
    1,
    Math.floor((contentWidth + POKEMON_GRID_GAP) / (POKEMON_CARD_MIN_WIDTH + POKEMON_GRID_GAP)),
  )
  const totalRows = Math.ceil(itemCount / columns)
  const visibleRows = Math.ceil(
    Math.max(0, viewportHeight - POKEMON_GRID_PADDING) / POKEMON_GRID_ROW_HEIGHT,
  )
  const firstVisibleRow = Math.floor(Math.max(0, scrollTop) / POKEMON_GRID_ROW_HEIGHT)
  const startRow = Math.min(
    totalRows,
    Math.max(0, firstVisibleRow - POKEMON_GRID_OVERSCAN_ROWS),
  )
  const endRow = Math.max(
    startRow,
    Math.min(totalRows, firstVisibleRow + visibleRows + POKEMON_GRID_OVERSCAN_ROWS),
  )

  return {
    columns,
    startIndex: Math.min(itemCount, startRow * columns),
    endIndex: Math.min(itemCount, endRow * columns),
    top: startRow * POKEMON_GRID_ROW_HEIGHT,
    totalHeight: totalRows * POKEMON_GRID_ROW_HEIGHT,
  }
}
