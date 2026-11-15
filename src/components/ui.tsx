import { animate, motion, useReducedMotion } from 'motion/react'
import { useEffect, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from 'react'
import { go } from '../lib/router'
import { usePrinting } from '../lib/print'
import { confLabel } from '../lib/format'
import { dimLabel, isValidated, type DimId } from '../engine/dimensions'
import type { Insight } from '../engine/types'

export const cx = (...a: (string | false | undefined | null)[]) => a.filter(Boolean).join(' ')

type BtnProps = ButtonHTMLAttributes<HTMLButtonElement> & { variant?: 'primary' | 'secondary' | 'ghost'; size?: 'md' | 'lg' }
export function Button({ variant = 'primary', size = 'md', className, ...p }: BtnProps) {
  const base =
    'inline-flex items-center justify-center gap-2 rounded-full font-medium transition-colors disabled:opacity-40 disabled:pointer-events-none select-none'
  const sizes = { md: 'px-5 py-2.5 text-[15px]', lg: 'px-7 py-3.5 text-[17px]' }
  const v = {
    primary: 'bg-accent text-white hover:brightness-110 active:brightness-95',
    secondary: 'bg-raised text-ink border border-line hover:bg-line/40',
    ghost: 'text-accent hover:bg-accent/10',
  }
  return <button type="button" className={cx(base, sizes[size], v[variant], className)} {...p} />
}

export function Card({ children, className, as: T = 'div' }: { children: ReactNode; className?: string; as?: 'div' | 'section' | 'article' }) {
  return <T className={cx('min-w-0 rounded-card border border-line bg-surface shadow-card p-5 sm:p-7 print-avoid', className)}>{children}</T>
}

export function Reveal({ children, className, delay = 0 }: { children: ReactNode; className?: string; delay?: number }) {
  const reduce = useReducedMotion()
  const printing = usePrinting()
  if (reduce || printing) return <div className={cx('min-w-0', className)}>{children}</div>
  return (
    <motion.div
      data-reveal
      className={cx('min-w-0', className)}
      initial={{ opacity: 0, y: 18 }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true, margin: '0px 0px -8% 0px' }}
      transition={{ duration: 0.6, delay, ease: [0.22, 1, 0.36, 1] }}
    >
      {children}
    </motion.div>
  )
}

export function Counter({ value, suffix = '', className }: { value: number; suffix?: string; className?: string }) {
  const reduce = useReducedMotion()
  const [v, setV] = useState(reduce ? value : 0)
  const ref = useRef<HTMLSpanElement>(null)
  useEffect(() => {
    if (reduce) {
      setV(value)
      return
    }
    const c = animate(0, value, { duration: 1.2, ease: 'easeOut', onUpdate: (x) => setV(x) })
    return () => c.stop()
  }, [value, reduce])
  return (
    <span ref={ref} className={cx('tabular-nums', className)}>
      {Math.round(v)}
      {suffix}
    </span>
  )
}

export function Badge({ children, tone = 'neutral', title }: { children: ReactNode; tone?: 'neutral' | 'good' | 'warn' | 'accent'; title?: string }) {
  const t = {
    neutral: 'bg-raised text-muted border-line',
    good: 'bg-good/10 text-good border-good/30',
    warn: 'bg-warn/10 text-warn border-warn/30',
    accent: 'bg-accent/10 text-accent border-accent/30',
  }
  return (
    <span title={title} className={cx('inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-medium whitespace-nowrap', t[tone])}>
      {children}
    </span>
  )
}

export function ConfBadge({ confidence }: { confidence: number }) {
  const l = confLabel(confidence)
  const tone = l === 'High' ? 'good' : l === 'Moderate' ? 'accent' : 'warn'
  return (
    <Badge tone={tone} title={`Confidence ${Math.round(confidence * 100)}%. How well your answers pin this down.`}>
      {l} confidence
    </Badge>
  )
}

export function ValidityBadge({ dim }: { dim: DimId }) {
  return isValidated(dim) ? (
    <Badge tone="good" title="Scored from a published, free-to-use instrument.">
      Validated instrument
    </Badge>
  ) : (
    <Badge title="A custom score for exploring, not a clinical or validated measure.">Exploratory</Badge>
  )
}

export function Eyebrow({ children }: { children: ReactNode }) {
  return <p className="text-[13px] font-semibold uppercase tracking-[0.12em] text-accent">{children}</p>
}

export function SectionHead({ id, letter, title, lead }: { id?: string; letter?: string; title: string; lead?: string }) {
  return (
    <div id={id} className="scroll-mt-28 mb-6">
      {letter && <Eyebrow>Section {letter}</Eyebrow>}
      <h2 className="mt-1 text-3xl sm:text-4xl font-semibold tracking-tight">{title}</h2>
      {lead && <p className="mt-2 text-lg text-muted max-w-2xl">{lead}</p>}
    </div>
  )
}

export function Paras({ items, className }: { items: string[]; className?: string }) {
  return (
    <div className={cx('space-y-3 text-[17px] leading-relaxed text-muted max-w-3xl', className)}>
      {items.map((p, i) => (
        <p key={i}>{p}</p>
      ))}
    </div>
  )
}

export function Bar({ value, label, right, tone = 'accent', sub }: { value: number; label: string; right?: ReactNode; tone?: 'accent' | 'good' | 'warn'; sub?: string }) {
  const rm = useReducedMotion()
  const pr = usePrinting()
  const reduce = rm || pr
  const col = { accent: 'bg-accent', good: 'bg-good', warn: 'bg-warn' }[tone]
  const v = Math.max(0, Math.min(100, value))
  return (
    <div>
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1 text-[15px]">
        <span className="font-medium">{label}</span>
        <span className="text-muted tabular-nums flex flex-wrap items-center gap-2">{right}{Math.round(v)}</span>
      </div>
      <div className="mt-1.5 h-2 rounded-full bg-line/70 overflow-hidden" role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(v)}>
        {reduce ? (
          <div className={cx('h-full rounded-full', col)} style={{ width: `${v}%` }} />
        ) : (
          <motion.div
            className={cx('h-full rounded-full', col)}
            initial={{ width: 0 }}
            whileInView={{ width: `${v}%` }}
            viewport={{ once: true }}
            transition={{ duration: 0.9, ease: [0.22, 1, 0.36, 1] }}
          />
        )}
      </div>
      {sub && <p className="mt-1 text-sm text-faint">{sub}</p>}
    </div>
  )
}

export function InsightCard({ i }: { i: Insight }) {
  return (
    <div className="rounded-2xl border border-line bg-raised p-4 print-avoid">
      <h4 className="font-semibold text-[17px] leading-snug">{i.title}</h4>
      <p className="mt-1.5 text-muted leading-relaxed">{i.body}</p>
      {i.evidence?.length > 0 && (
        <p className="mt-2.5 text-xs text-faint">
          Based on: {i.evidence.map((d) => dimLabel(d)).join(', ')}
        </p>
      )}
    </div>
  )
}

export function InsightGrid({ items, cols = 2 }: { items: Insight[]; cols?: 1 | 2 | 3 }) {
  if (!items?.length) return null
  const c = { 1: '', 2: 'md:grid-cols-2', 3: 'md:grid-cols-2 lg:grid-cols-3' }[cols]
  return (
    <div className={cx('grid gap-3', c)}>
      {items.map((i, k) => (
        <InsightCard key={k} i={i} />
      ))}
    </div>
  )
}

export function SubHead({ children, note }: { children: ReactNode; note?: string }) {
  return (
    <div className="mt-8 mb-3">
      <h3 className="text-xl font-semibold tracking-tight">{children}</h3>
      {note && <p className="text-sm text-faint mt-0.5">{note}</p>}
    </div>
  )
}

const NAV: [string, string][] = [
  ['/results', 'Results'],
  ['/explore', 'Explorer'],
  ['/try', 'What to try'],
  ['/simulator', 'Simulator'],
  ['/contradictions', 'Contradictions'],
  ['/growth', 'Growth'],
  ['/share', 'Share'],
  ['/settings', 'Settings'],
]

export function Shell({ path, children, wide }: { path: string; children: ReactNode; wide?: boolean }) {
  return (
    <div className="min-h-screen">
      <header className="no-print sticky top-0 z-30 border-b border-line bg-bg/80 backdrop-blur-md">
        <div className="mx-auto max-w-6xl px-4 sm:px-6 flex items-center gap-4 h-14">
          <a href="#/" className="font-semibold tracking-tight shrink-0">
            The Human Index
          </a>
          <nav aria-label="Main" className="flex-1 overflow-x-auto [scrollbar-width:none]">
            <ul className="flex gap-1 w-max ml-auto">
              {NAV.map(([p, l]) => {
                const on = path === p || (p === '/explore' && path.startsWith('/explore'))
                return (
                  <li key={p}>
                    <a
                      href={`#${p}`}
                      aria-current={on ? 'page' : undefined}
                      className={cx('block rounded-full px-3 py-1.5 text-sm whitespace-nowrap', on ? 'bg-ink text-bg' : 'text-muted hover:text-ink hover:bg-line/50')}
                    >
                      {l}
                    </a>
                  </li>
                )
              })}
            </ul>
          </nav>
        </div>
      </header>
      <main className={cx('mx-auto px-4 sm:px-6 py-8 sm:py-12', wide ? 'max-w-6xl' : 'max-w-4xl')}>{children}</main>
      <footer className="no-print mx-auto max-w-4xl px-4 sm:px-6 pb-12 text-sm text-faint">
        Everything here stays on this device. This is a tool for reflection, not a diagnosis.
      </footer>
    </div>
  )
}

export function PageTitle({ eyebrow, title, lead, children }: { eyebrow?: string; title: string; lead?: string; children?: ReactNode }) {
  return (
    <div className="mb-8 sm:mb-10">
      {eyebrow && <Eyebrow>{eyebrow}</Eyebrow>}
      <h1 className="mt-1 text-4xl sm:text-5xl font-semibold tracking-tight">{title}</h1>
      {lead && <p className="mt-3 text-lg sm:text-xl text-muted max-w-2xl">{lead}</p>}
      {children}
    </div>
  )
}

export function Empty({ title, body, cta }: { title: string; body: string; cta?: { label: string; to: string } }) {
  return (
    <Card className="text-center py-12">
      <h2 className="text-2xl font-semibold">{title}</h2>
      <p className="mt-2 text-muted max-w-md mx-auto">{body}</p>
      {cta && (
        <div className="mt-6">
          <Button onClick={() => go(cta.to)}>{cta.label}</Button>
        </div>
      )}
    </Card>
  )
}
