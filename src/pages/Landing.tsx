import { motion, useReducedMotion } from 'motion/react'
import { SECTIONS } from '../data/sections'
import { loadProgress, clearProgress } from '../engine/storage'
import { Button } from '../components/ui'
import { go } from '../lib/router'
import type { Report } from '../engine/types'

export default function Landing({ report }: { report: Report | null }) {
  const reduce = useReducedMotion()
  const mins = SECTIONS.reduce((t, x) => t + x.estMinutes, 0)
  const p = loadProgress()
  const hasProgress = !!p && Object.keys(p.answers).length > 0
  const a = (d: number) => (reduce ? {} : { initial: { opacity: 0, y: 20 }, animate: { opacity: 1, y: 0 }, transition: { duration: 0.8, delay: d, ease: [0.22, 1, 0.36, 1] as const } })
  return (
    <div className="relative min-h-screen overflow-hidden">
      <div aria-hidden className="pointer-events-none absolute inset-x-0 top-0 h-[80vh] bg-[radial-gradient(ellipse_70%_60%_at_50%_0%,rgb(var(--accent)/0.20),transparent_70%)]" />
      <header className="relative mx-auto max-w-5xl px-5 sm:px-8 h-16 flex items-center justify-between">
        <span className="font-semibold tracking-tight">The Human Index</span>
        <a href="#/settings" className="text-sm text-muted hover:text-ink rounded px-2 py-1">Settings</a>
      </header>
      <main className="relative mx-auto max-w-4xl px-5 sm:px-8 pt-16 sm:pt-28 pb-24 text-center">
        <motion.p {...a(0)} className="text-[13px] font-semibold uppercase tracking-[0.14em] text-accent">A questionnaire about you</motion.p>
        <motion.h1 {...a(0.08)} className="mt-4 text-5xl sm:text-7xl md:text-[88px] font-semibold tracking-tight leading-[1.02]">
          Find out what<br />makes you, you.
        </motion.h1>
        <motion.p {...a(0.18)} className="mx-auto mt-7 max-w-2xl text-xl sm:text-2xl text-muted leading-snug">
          Answer questions about how you think, what drives you and what you enjoy. Get a clear, honest picture of your strengths, your tensions and what to explore next.
        </motion.p>
        <motion.div {...a(0.28)} className="mt-10 flex flex-col sm:flex-row items-center justify-center gap-3">
          {hasProgress ? (
            <>
              <Button size="lg" onClick={() => go('/assess')}>Resume</Button>
              <Button size="lg" variant="secondary" onClick={() => { clearProgress(); go('/intro') }}>Start over</Button>
            </>
          ) : (
            <Button size="lg" onClick={() => go('/intro')}>Start</Button>
          )}
          {report && <Button size="lg" variant={hasProgress ? 'ghost' : 'secondary'} onClick={() => go('/results')}>View results</Button>}
        </motion.div>

        <motion.div {...a(0.4)} className="mx-auto mt-20 max-w-2xl rounded-card border border-line bg-surface/70 p-6 sm:p-8 text-left backdrop-blur-sm">
          <h2 className="text-xl font-semibold">Private by design</h2>
          <p className="mt-2 text-muted leading-relaxed">
            Everything stays on this device. Your answers and results are saved in your browser only. There is no account, no server, and nothing is ever uploaded. You can export or delete all of it whenever you like.
          </p>
        </motion.div>
        <motion.div {...a(0.5)} className="mx-auto mt-4 grid max-w-2xl gap-4 sm:grid-cols-3 text-left">
          {[['Takes a while', `About ${mins} minutes. You can stop and pick up later.`], ['Honest, not final', 'It is a mirror for reflection. It is not a diagnosis.'], ['Built to explain', 'Every result shows the answers behind it.']].map(([t, b]) => (
            <div key={t} className="rounded-2xl border border-line bg-surface/70 p-5 backdrop-blur-sm"><h3 className="font-semibold">{t}</h3><p className="mt-1 text-sm text-muted">{b}</p></div>
          ))}
        </motion.div>
      </main>
    </div>
  )
}
