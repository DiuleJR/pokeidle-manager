import type { ButtonHTMLAttributes, ComponentPropsWithoutRef, ReactNode } from 'react'

export function Card({ children, className = '', ...props }: ComponentPropsWithoutRef<'section'>) {
  return (
    <section className={`card ${className}`} {...props}>
      {children}
    </section>
  )
}

export function Button({
  children,
  className = '',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button className={`button ${className}`} {...props}>
      {children}
    </button>
  )
}

export function Switch({
  checked,
  onCheckedChange,
  label,
  disabled = false,
}: {
  checked: boolean
  onCheckedChange: (value: boolean) => void
  label: string
  disabled?: boolean
}) {
  const change = () => onCheckedChange(!checked)

  return (
    <button
      type="button"
      className="switch"
      role="switch"
      aria-label={label}
      aria-checked={checked}
      disabled={disabled}
      onClick={change}
      onKeyDown={(event) => {
        if (event.key === ' ' || event.key === 'Enter') {
          // Prevent the browser's synthetic click so this keyboard action toggles exactly once.
          event.preventDefault()
          change()
        }
      }}
    >
      <span className="switch-label">{label}</span>
      <span className="switch-track" aria-hidden="true">
        <span className="switch-thumb" />
      </span>
    </button>
  )
}

export function Badge({
  children,
  tone = 'neutral',
}: {
  children: ReactNode
  tone?: 'neutral' | 'positive' | 'warning' | 'danger'
}) {
  return <span className={`badge badge-${tone}`}>{children}</span>
}

export function EmptyState({ onAdd }: { onAdd: () => void }) {
  return (
    <div className="empty-state">
      <div className="empty-orb">◌</div>
      <h2>Nenhuma conta cadastrada</h2>
      <p>Conecte uma conta pelo Brave para começar a acompanhar sua farm.</p>
      <Button onClick={onAdd}>+ Adicionar conta</Button>
    </div>
  )
}
