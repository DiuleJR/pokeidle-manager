export type PokemonSpriteFrame = {
  assetPath: string
  x: number
  y: number
  width: number
  height: number
  pageWidth: number
  pageHeight: number
}

export function createPokemonSpriteResolver(
  resolveLooktype: (looktype: number) => Promise<PokemonSpriteFrame | null>,
) {
  const requests = new Map<number, Promise<PokemonSpriteFrame | null>>()

  return async (looktypes: number[]): Promise<PokemonSpriteFrame | null> => {
    for (const looktype of new Set(looktypes)) {
      let request = requests.get(looktype)
      if (!request) {
        request = resolveLooktype(looktype).catch((error: unknown) => {
          requests.delete(looktype)
          throw error
        })
        requests.set(looktype, request)
      }
      const frame = await request
      if (frame) return frame
    }
    return null
  }
}
