import { motion, useReducedMotion } from 'motion/react'
import { useMemo, useState } from 'react'
import { dimLabel, isInverse, type DimId } from '../engine/dimensions'
import type { Report } from '../engine/types'
import { useColors } from './useColors'
import { Badge, Button, ConfBadge, cx } from './ui'
import { go } from '../lib/router'

interface NodeDef {
  key: string
  label: string
  x: number
  y: number
  dims: DimId[]
  blurb: string
  override?: (r: Report) => number
}

const NODES: NodeDef[] = [
  { key: 'personality', label: 'Personality', x: 110, y: 130, dims: ['openness', 'conscientiousness', 'extraversion', 'agreeableness', 'emotionalStability'], blurb: 'The five broad traits, averaged. Open the Big Five below for the detail.' },
  { key: 'curiosity', label: 'Curiosity', x: 495, y: 95, dims: ['curiosity', 'intellectualDrive', 'openness', 'masteryOrientation'], blurb: 'How strongly you want to understand things and keep learning.' },
  { key: 'creativity', label: 'Creativity', x: 85, y: 285, dims: ['creativity', 'motCreativity', 'artistic', 'valCreativity'], blurb: 'Your pull towards making, imagining and original ideas.' },
  { key: 'ambition', label: 'Ambition', x: 515, y: 255, dims: ['achievement', 'growthDrive', 'influence', 'financialDrive', 'recognitionNeed'], blurb: 'How much drive you bring to getting somewhere, and what kind.' },
  { key: 'confidence', label: 'Confidence', x: 120, y: 440, dims: ['selfEsteem', 'selfEfficacy', 'socialConfidence', 'professionalConfidence'], blurb: 'How capable and settled you feel in yourself and around others.' },
  { key: 'motivation', label: 'Motivation', x: 505, y: 415, dims: ['intrinsicMotivation', 'engagement', 'goalClarity', 'futureOptimism'], blurb: 'The inner fuel: interest, clear goals and hope for what is ahead.' },
  { key: 'relationships', label: 'Relationships', x: 95, y: 590, dims: ['relationshipQuality', 'trust', 'empathy', 'communication', 'relatedness', 'feelingUnderstood', 'loneliness'], blurb: 'How connected, trusting and understood you feel with other people.' },
  { key: 'career', label: 'Career', x: 500, y: 575, dims: ['careerFit', 'careerSatisfaction', 'careerClarity', 'professionalConfidence'], blurb: 'How well your work direction fits who you seem to be.', override: (r) => r.career.alignmentScore },
  { key: 'discipline', label: 'Discipline', x: 190, y: 715, dims: ['conscientiousness', 'consistency', 'followThrough', 'impulseControl', 'timeManagement', 'procrastination'], blurb: 'Following through, staying steady, and managing your own time.' },
  { key: 'happiness', label: 'Happiness', x: 415, y: 720, dims: ['lifeSatisfaction', 'positiveEmotion', 'meaning', 'optimism'], blurb: 'Your overall sense of how life is going.', override: (r) => r.happiness.overall },
  { key: 'values', label: 'Values', x: 300, y: 330, dims: ['valHonesty', 'valFamily', 'valFreedom', 'valAchievement', 'valCompassion'], blurb: 'What you hold most dear. Your top values are listed below.', override: (r) => (r.values.ranked.slice(0, 5).reduce((s, v) => s + v.score, 0) / Math.max(1, r.values.ranked.slice(0, 5).length)) },
]

const BODY =
  'M300 186C262 186 226 198 212 232C202 258 200 300 196 360C194 385 216 388 220 365C226 320 230 285 236 270L236 420C236 470 232 540 228 600C226 640 226 660 232 676C244 684 276 684 284 672C290 640 296 560 300 520C304 560 310 640 316 672C324 684 356 684 368 676C374 660 374 640 372 600C368 540 364 470 364 420L364 270C370 285 374 320 380 365C384 388 406 385 404 360C400 300 398 258 388 232C374 198 338 186 300 186Z'

export function nodeValue(n: NodeDef, r: Report) {
  const vals = n.dims.map((d) => {
    const s = r.scores[d]
    return { d, score: isInverse(d) ? 100 - s.score : s.score, conf: s.confidence, n: s.n }
  })
  const measured = vals.filter((v) => v.n > 0)
  const src = measured.length ? measured : vals
  const avg = src.reduce((a, v) => a + v.score, 0) / Math.max(1, src.length)
  const conf = src.reduce((a, v) => a + v.conf, 0) / Math.max(1, src.length)
  return { value: n.override ? n.override(r) : avg, conf, vals: [...vals].sort((a, b) => b.score - a.score) }
}

export function HumanViz({ report }: { report: Report }) {
  const reduce = useReducedMotion()
  const c = useColors()
  const [hover, setHover] = useState<string | null>(null)
  const [open, setOpen] = useState<string | null>(null)
  const data = useMemo(() => NODES.map((n) => ({ n, ...nodeValue(n, report) })), [report])
  const centre = { x: 300, y: 330 }
  const links = useMemo(() => {
    const pairs: [string, string][] = [
      ['personality', 'curiosity'], ['personality', 'creativity'], ['curiosity', 'ambition'], ['creativity', 'confidence'],
      ['ambition', 'motivation'], ['confidence', 'relationships'], ['motivation', 'career'], ['relationships', 'discipline'],
      ['career', 'happiness'], ['discipline', 'happiness'],
    ]
    return pairs
  }, [])
  const byKey = Object.fromEntries(data.map((d) => [d.n.key, d]))
  const active = open ? byKey[open] : null
  const hov = hover ? byKey[hover] : null

  return (
    <div>
      <div className="relative mx-auto w-full max-w-[560px]">
        <svg viewBox="0 0 600 780" role="group" aria-label="Your profile as a human figure with eleven connected points" className="w-full h-auto select-none">
          <defs>
            <radialGradient id="hv-glow">
              <stop offset="0%" stopColor={c.accent} stopOpacity="0.9" />
              <stop offset="100%" stopColor={c.accent} stopOpacity="0" />
            </radialGradient>
            <linearGradient id="hv-body" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={c.accent} stopOpacity="0.16" />
              <stop offset="100%" stopColor={c.accent} stopOpacity="0.03" />
            </linearGradient>
          </defs>

          <g aria-hidden>
            <motion.circle cx="300" cy="128" r="48" fill="url(#hv-body)" stroke={c.faint} strokeOpacity="0.5" strokeWidth="1.5" initial={reduce ? false : { opacity: 0, scale: 0.9 }} animate={{ opacity: 1, scale: 1 }} style={{ transformOrigin: '300px 128px' }} transition={{ duration: 1 }} />
            <motion.path d={BODY} fill="url(#hv-body)" stroke={c.faint} strokeOpacity="0.5" strokeWidth="1.5" strokeLinejoin="round" initial={reduce ? false : { opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 1.2, delay: 0.1 }} />
            <circle cx="300" cy="330" r="130" fill="url(#hv-glow)" opacity="0.12" />
          </g>

          {/* spokes to the heart */}
          <g aria-hidden>
            {data.filter((d) => d.n.key !== 'values').map(({ n, value }, i) => (
              <motion.line
                key={n.key}
                x1={centre.x} y1={centre.y} x2={n.x} y2={n.y}
                stroke={c.accent}
                strokeWidth={hover === n.key || open === n.key ? 2 : 1}
                strokeOpacity={hover === n.key || open === n.key ? 0.8 : 0.18 + (value / 100) * 0.25}
                initial={reduce ? false : { pathLength: 0 }}
                animate={{ pathLength: 1 }}
                transition={{ duration: 1.1, delay: 0.4 + i * 0.07, ease: 'easeOut' }}
              />
            ))}
            {links.map(([a, b], i) => (
              <motion.line
                key={a + b}
                x1={byKey[a].n.x} y1={byKey[a].n.y} x2={byKey[b].n.x} y2={byKey[b].n.y}
                stroke={c.accent} strokeWidth="1" strokeOpacity="0.28"
                strokeDasharray="3 7"
                className={reduce ? undefined : 'hv-flow'}
                initial={reduce ? false : { opacity: 0 }}
                animate={{ opacity: 1 }}
                transition={{ duration: 1, delay: 1 + i * 0.08 }}
              />
            ))}
          </g>

          {data.map(({ n, value }, i) => {
            const r = 9 + (value / 100) * 15
            const op = 0.45 + (value / 100) * 0.55
            const isOn = hover === n.key || open === n.key
            const labelRight = n.x < 300
            return (
              <g
                key={n.key}
                role="button"
                tabIndex={0}
                aria-label={`${n.label}, ${Math.round(value)} out of 100. Show details`}
                aria-pressed={open === n.key}
                className="cursor-pointer outline-none [&:focus-visible>circle.ring]:stroke-[3]"
                onMouseEnter={() => setHover(n.key)}
                onMouseLeave={() => setHover(null)}
                onFocus={() => setHover(n.key)}
                onBlur={() => setHover(null)}
                onClick={() => setOpen(open === n.key ? null : n.key)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault()
                    setOpen(open === n.key ? null : n.key)
                  }
                }}
              >
                <motion.g
                  initial={reduce ? false : { opacity: 0, scale: 0.4 }}
                  animate={{ opacity: 1, scale: 1 }}
                  style={{ transformOrigin: `${n.x}px ${n.y}px` }}
                  transition={{ duration: 0.7, delay: 0.6 + i * 0.09, type: 'spring', bounce: 0.35 }}
                >
                  <circle cx={n.x} cy={n.y} r={r * 3.2} fill="url(#hv-glow)" opacity={op * (isOn ? 0.7 : 0.4)} />
                  <motion.circle
                    className="ring"
                    cx={n.x} cy={n.y} r={r}
                    fill={c.accent} fillOpacity={op}
                    stroke={c.accent} strokeWidth={isOn ? 3 : 1.5} strokeOpacity={0.9}
                    animate={reduce ? undefined : { r: [r, r * 1.08, r] }}
                    transition={{ duration: 3 + (i % 4) * 0.5, repeat: Infinity, ease: 'easeInOut', delay: i * 0.2 }}
                  />
                  <circle cx={n.x} cy={n.y} r={36} fill="transparent" />
                  <text
                    x={n.x + (labelRight ? -1 : 1) * (r + 10) * 0 }
                    y={n.y + r + 22}
                    textAnchor="middle"
                    fontSize="19" fontWeight={isOn ? 600 : 500}
                    fill={isOn ? c.ink : c.muted}
                  >
                    {n.label}
                  </text>
                </motion.g>
              </g>
            )
          })}
        </svg>

        {hov && !open && (
          <div
            role="tooltip"
            className="pointer-events-none absolute z-10 w-44 -translate-x-1/2 rounded-xl border border-line bg-surface/95 p-3 text-sm shadow-card backdrop-blur"
            style={{ left: `${Math.min(80, Math.max(20, (hov.n.x / 600) * 100))}%`, top: `${((hov.n.y + 34) / 780) * 100}%` }}
          >
            <p className="font-semibold">{hov.n.label}</p>
            <p className="text-muted tabular-nums">{Math.round(hov.value)} / 100</p>
            <p className="text-xs text-faint mt-1">{hov.n.blurb}</p>
          </div>
        )}
      </div>

      <div className="mt-4 min-h-[1px]" aria-live="polite">
        {active ? (
          <motion.div key={active.n.key} initial={reduce ? false : { opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} className="rounded-card border border-line bg-surface shadow-card p-5 sm:p-6 text-left">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <h3 className="text-2xl font-semibold tracking-tight">{active.n.label}</h3>
                <p className="mt-1 text-muted max-w-xl">{active.n.blurb}</p>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-4xl font-semibold tabular-nums">{Math.round(active.value)}</span>
                <ConfBadge confidence={active.conf} />
              </div>
            </div>
            <ul className="mt-4 grid gap-x-8 gap-y-2.5 sm:grid-cols-2">
              {active.vals.slice(0, 6).map((v) => (
                <li key={v.d}>
                  <button type="button" onClick={() => go(`/explore/${v.d}`)} className="w-full text-left group">
                    <div className="flex justify-between text-[15px]">
                      <span className="group-hover:text-accent">{dimLabel(v.d)}{isInverse(v.d) ? ' (reversed)' : ''}</span>
                      <span className="tabular-nums text-muted">{Math.round(v.score)}</span>
                    </div>
                    <div className="mt-1 h-1.5 rounded-full bg-line/70 overflow-hidden"><div className="h-full rounded-full bg-accent" style={{ width: `${v.score}%` }} /></div>
                  </button>
                </li>
              ))}
            </ul>
            <div className="mt-4 flex gap-2 flex-wrap">
              <Button variant="secondary" onClick={() => setOpen(null)}>Close</Button>
              <Badge>Scores are 0 to 100 and describe your answers, not a verdict</Badge>
            </div>
          </motion.div>
        ) : (
          <p className={cx('text-center text-sm text-faint')}>Tap or click any point to see what sits behind it.</p>
        )}
      </div>
    </div>
  )
}
