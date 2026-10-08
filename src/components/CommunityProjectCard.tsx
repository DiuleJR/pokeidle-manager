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

// Marcas SVG do Simple Icons (CC0); a procedência está registrada em docs/ASSETS.md.
function GitHubMark() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12" />
    </svg>
  )
}

function DiscordMark() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M20.317 4.3698a19.7913 19.7913 0 00-4.8851-1.5152.0741.0741 0 00-.0785.0371c-.211.3753-.4447.8648-.6083 1.2495-1.8447-.2762-3.68-.2762-5.4868 0-.1636-.3933-.4058-.8742-.6177-1.2495a.077.077 0 00-.0785-.037 19.7363 19.7363 0 00-4.8852 1.515.0699.0699 0 00-.0321.0277C.5334 9.0458-.319 13.5799.0992 18.0578a.0824.0824 0 00.0312.0561c2.0528 1.5076 4.0413 2.4228 5.9929 3.0294a.0777.0777 0 00.0842-.0276c.4616-.6304.8731-1.2952 1.226-1.9942a.076.076 0 00-.0416-.1057c-.6528-.2476-1.2743-.5495-1.8722-.8923a.077.077 0 01-.0076-.1277c.1258-.0943.2517-.1923.3718-.2914a.0743.0743 0 01.0776-.0105c3.9278 1.7933 8.18 1.7933 12.0614 0a.0739.0739 0 01.0785.0095c.1202.099.246.1981.3728.2924a.077.077 0 01-.0066.1276 12.2986 12.2986 0 01-1.873.8914.0766.0766 0 00-.0407.1067c.3604.698.7719 1.3628 1.225 1.9932a.076.076 0 00.0842.0286c1.961-.6067 3.9495-1.5219 6.0023-3.0294a.077.077 0 00.0313-.0552c.5004-5.177-.8382-9.6739-3.5485-13.6604a.061.061 0 00-.0312-.0286zM8.02 15.3312c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9555-2.4189 2.157-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.9555 2.4189-2.1569 2.4189zm7.9748 0c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9554-2.4189 2.1569-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.946 2.4189-2.1568 2.4189Z" />
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
          <span className="community-link-identity">
            <span className="community-link-icon community-link-icon-github" aria-hidden="true">
              <GitHubMark />
            </span>
            <span className="community-link-copy">
              <small className="community-link-kicker">REPOSITÓRIO OFICIAL</small>
              <strong>GitHub</strong>
            </span>
          </span>
          <span className="community-link-destination">github.com/DiuleJR/pokeidle-manager</span>
          <span className="community-link-cta">
            Ver repositório <ExternalLinkIcon />
          </span>
        </button>

        <button
          className="community-link-card"
          type="button"
          aria-label="Entrar na comunidade Pokeidle Manager no Discord"
          onClick={() => openCommunityUrl(COMMUNITY.discordUrl)}
        >
          <span className="community-link-identity">
            <span className="community-link-icon community-link-icon-discord" aria-hidden="true">
              <DiscordMark />
            </span>
            <span className="community-link-copy">
              <small className="community-link-kicker">COMUNIDADE OFICIAL</small>
              <strong>Discord</strong>
            </span>
          </span>
          <span className="community-link-destination">{COMMUNITY.discordInvite}</span>
          <span className="community-link-cta">
            Entrar na comunidade <ExternalLinkIcon />
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
