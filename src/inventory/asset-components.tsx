import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import type { DepotPokemon, InventoryItem } from '../types'
import { ItemAssetResolver, PokemonAssetResolver, type GameItemCatalog } from './assets'
import { createPokemonSpriteResolver, type PokemonSpriteFrame } from './sprite-cache'

type ResolvedAsset = { path: string }
type SpriteFrame = PokemonSpriteFrame

const assetRequests = new Map<string, Promise<string | null>>()
const resolvePokemonSprite = createPokemonSpriteResolver((looktype) =>
  invoke<SpriteFrame | null>('resolve_pokemon_sprite', { looktypes: [looktype] }),
)

function inTauri() {
  return '__TAURI_INTERNALS__' in window
}

function verifyImageUrl(url: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const image = new Image()
    image.onload = () => resolve(url)
    image.onerror = () => reject(new Error('Não foi possível carregar o asset local.'))
    image.src = url
  })
}

function useAssetUrl(assetPath: string | null | undefined) {
  const [url, setUrl] = useState<string | null>(null)
  useEffect(() => {
    let active = true
    if (!assetPath || !inTauri()) {
      setUrl(null)
      return () => {
        active = false
      }
    }
    let request = assetRequests.get(assetPath)
    if (!request) {
      request = invoke<ResolvedAsset>('resolve_game_asset', { assetPath })
        .then((asset) => verifyImageUrl(convertFileSrc(asset.path)))
        .catch(() => {
          // A resposta negativa do Rust tem TTL. Não retenha uma falha de
          // transporte no cliente para sempre: uma próxima montagem pode tentar
          // de novo depois de o recurso voltar a ficar disponível.
          assetRequests.delete(assetPath)
          return null
        })
      assetRequests.set(assetPath, request)
    }
    void request.then((value) => {
      if (active) setUrl(value)
    })
    return () => {
      active = false
    }
  }, [assetPath])
  return url
}

function AssetFallback({ label, className = '' }: { label: string; className?: string }) {
  return (
    <span className={`asset-fallback ${className}`.trim()} aria-label={`${label}: sem imagem`}>
      ?
    </span>
  )
}

export function ItemAsset({
  item,
  catalog,
  compact = false,
}: {
  item: InventoryItem
  catalog: GameItemCatalog
  compact?: boolean
}) {
  const source = ItemAssetResolver.assetPath(item, catalog)
  const url = useAssetUrl(source)
  const [failed, setFailed] = useState(false)
  useEffect(() => setFailed(false), [url])
  if (!url || failed)
    return <AssetFallback label={item.name} className={compact ? 'asset-compact' : ''} />
  return (
    <img
      className={`resolved-asset ${compact ? 'asset-compact' : ''}`.trim()}
      src={url}
      alt=""
      onError={() => setFailed(true)}
    />
  )
}

function useSpriteFrame(key: string) {
  const [frame, setFrame] = useState<SpriteFrame | null>(null)
  useEffect(() => {
    let active = true
    if (!key || !inTauri()) {
      setFrame(null)
      return () => {
        active = false
      }
    }
    void resolvePokemonSprite(key.split(',').map(Number).filter(Number.isFinite))
      .then((value) => {
        if (active) setFrame(value)
      })
      .catch(() => {
        if (active) setFrame(null)
      })
    return () => {
      active = false
    }
  }, [key])
  return frame
}

export function PokemonAsset({
  pokemon,
  catalog,
  size = 68,
}: {
  pokemon: DepotPokemon
  catalog: GameItemCatalog
  size?: number
}) {
  const visualSize = Math.max(32, size)
  const preferred = PokemonAssetResolver.preferredLooktypes(pokemon)
  const atlas = PokemonAssetResolver.atlasSlot(pokemon, catalog.markerAtlas)
  const atlasUrl = useAssetUrl(atlas ? catalog.markerAtlas.assetPath : null)
  const frame = useSpriteFrame(atlas ? '' : preferred.join(','))
  const frameUrl = useAssetUrl(frame?.assetPath)
  if (atlas && atlasUrl) {
    const [column, row] = atlas.slot
    return (
      <span
        className="pokemon-asset pokemon-atlas"
        role="img"
        aria-label={pokemon.name}
        style={{
          width: visualSize,
          height: visualSize,
          backgroundImage: `url("${atlasUrl}")`,
          backgroundSize: `${catalog.markerAtlas.cols * 100}% ${catalog.markerAtlas.rows * 100}%`,
          backgroundPosition: `${(column / Math.max(1, catalog.markerAtlas.cols - 1)) * 100}% ${(row / Math.max(1, catalog.markerAtlas.rows - 1)) * 100}%`,
        }}
      />
    )
  }
  if (frame && frameUrl) {
    const scale = Math.min((visualSize - 6) / frame.width, (visualSize - 6) / frame.height)
    return (
      <span
        className="pokemon-asset-frame"
        role="img"
        aria-label={pokemon.name}
        style={{ width: visualSize, height: visualSize }}
      >
        <span
          style={{
            width: frame.width * scale,
            height: frame.height * scale,
            backgroundImage: `url("${frameUrl}")`,
            backgroundSize: `${frame.pageWidth * scale}px ${frame.pageHeight * scale}px`,
            backgroundPosition: `-${frame.x * scale}px -${frame.y * scale}px`,
          }}
        />
      </span>
    )
  }
  return <AssetFallback label={pokemon.name} className="pokemon-asset-fallback" />
}
