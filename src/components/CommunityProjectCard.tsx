import { memo, useState } from 'react'
import { openUrl } from '@tauri-apps/plugin-opener'
import { QRCodeSVG } from 'qrcode.react'
import { COMMUNITY } from '../config/community'
import { Badge, Button, Card } from './primitives'

const CommunityPixQr = memo(function CommunityPixQr() {
  return (
    <QRCodeSVG
      className="community-pix-qr"
      value={COMMUNITY.pixPayload}
      size={112}
      level="M"
      marginSize={4}
      bgColor="#fffaf2"
      fgColor="#21151d"
      title="QR Code para apoio voluntário via PIX"
      role="img"
      aria-label="QR Code para apoio voluntário via PIX"
    />
  )
})

function SourceIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="m8 6-6 6 6 6M16 6l6 6-6 6M14 3l-4 18" />
    </svg>
  )
}

function CommunityIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4 5.5h16v11H13l-4.5 3v-3H4zM8 10h.01M12 10h.01M16 10h.01" />
    </svg>
  )
}

function ExternalLinkIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <path d="M6 3h7v7M13 3 6.5 9.5M11 9v4H3V5h4" />
    </svg>
  )
}

export function CommunityProjectCard() {
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'error'>('idle')
  const [linkError, setLinkError] = useState(false)

  const openCommunityUrl = (url: string) => {
    setLinkError(false)
    void openUrl(url).catch(() => setLinkError(true))
  }

  const copyPix = async () => {
    try {
      if (!navigator.clipboard?.writeText) throw new Error('Clipboard indisponível')
      await navigator.clipboard.writeText(COMMUNITY.pixPayload)
      setCopyState('copied')
    } catch {
      setCopyState('error')
    }
  }

  const resetCopyFeedback = () => {
    if (copyState === 'copied') setCopyState('idle')
  }

  return (
    <Card className="community-project-card">
      <div className="community-project-heading">
        <div>
          <p className="eyebrow">PROJETO OPEN SOURCE</p>
          <h2>Pokeidle Manager</h2>
          <p className="community-project-version">Versão {COMMUNITY.version}</p>
        </div>
        <Badge tone="positive">Community Build</Badge>
      </div>

      <div className="community-project-status" aria-label="Sobre o projeto">
        <span className="community-chip community-chip-free">
          <span aria-hidden="true">✓</span> Gratuito
        </span>
        <span className="community-chip community-chip-license">
          <span aria-hidden="true">◇</span> Código aberto · GPL-3.0-only
        </span>
        <span className="community-chip community-chip-community">
          <span aria-hidden="true">✓</span> Sem ativação comercial
        </span>
      </div>

      <div className="community-project-panels">
        <button
          className="community-link-card"
          type="button"
          aria-label="Abrir o código-fonte do Pokeidle Manager no GitHub"
          onClick={() => openCommunityUrl(COMMUNITY.githubUrl)}
        >
          <span className="community-link-icon" aria-hidden="true">
            <SourceIcon />
          </span>
          <span className="community-link-copy">
            <strong>GitHub</strong>
            <span>Ver código-fonte</span>
            <small>github.com/DiuleJR</small>
          </span>
          <span className="community-link-arrow">
            <ExternalLinkIcon />
          </span>
        </button>

        <button
          className="community-link-card"
          type="button"
          aria-label="Entrar na comunidade Pokeidle Manager no Discord"
          onClick={() => openCommunityUrl(COMMUNITY.discordUrl)}
        >
          <span className="community-link-icon community-link-icon-discord" aria-hidden="true">
            <CommunityIcon />
          </span>
          <span className="community-link-copy">
            <strong>Discord</strong>
            <span>Entrar na comunidade</span>
            <small>{COMMUNITY.discordInvite}</small>
          </span>
          <span className="community-link-arrow">
            <ExternalLinkIcon />
          </span>
        </button>

        <section className="community-pix-panel" aria-labelledby="community-pix-title">
          <div className="community-pix-qr-frame">
            <CommunityPixQr />
          </div>
          <div className="community-pix-copy">
            <p className="community-pix-eyebrow">APOIE O PROJETO</p>
            <h3 id="community-pix-title">Apoio voluntário</h3>
            <p>Qualquer valor ajuda 💜</p>
            <small className="community-pix-note">Sem valor fixo · não libera funções</small>
            <Button
              className={`community-pix-copy-button ${copyState === 'copied' ? 'is-copied' : ''}`.trim()}
              type="button"
              aria-live="polite"
              onClick={() => void copyPix()}
              onAnimationEnd={resetCopyFeedback}
            >
              {copyState === 'copied' ? 'PIX copiado!' : 'Copiar PIX'}
            </Button>
          </div>
        </section>
      </div>

      <p className="community-project-feedback" aria-live="polite">
        {copyState === 'error' && 'Não foi possível copiar o PIX.'}
        {linkError && 'Não foi possível abrir o link no navegador.'}
      </p>
    </Card>
  )
}
