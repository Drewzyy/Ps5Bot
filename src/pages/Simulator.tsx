import { useEffect, useMemo, useState } from 'react'
import { simulateLifestyle } from '../engine/simulator'
import { loadPriorities, savePriorities } from '../engine/storage'
import type { LifePriorities, Report } from '../engine/types'
import { Badge, Bar, Card, PageTitle } from '../components/ui'

const FIELDS: { k: keyof LifePriorities; label: string; hint: string }[] = [
  { k: 'wealth', label: 'Wealth', hint: 'Money and financial strength' },
  { k: 'freedom', label: 'Freedom', hint: 'Choosing your own time and path' },
  { k: 'relationships', label: 'Relationships', hint: 'Family, friends, partner' },
  { k: 'achievement', label: 'Achievement', hint: 'Getting somewhere, mastering things' },
  { k: 'creativity', label: 'Creativity', hint: 'Making and imagining' },
  { k: 'stability', label: 'Stability', hint: 'Predictability and security' },
  { k: 'adventure', label: 'Adventure', hint: 'New places and new experiences' },
  { k: 'contribution', label: 'Contribution', hint: 'Being useful to others' },
]

function defaults(r: Report): LifePriorities {
  const s = (d: keyof Report['scores']) => Math.round(r.scores[d].score / 10)
  return {
    wealth: s('financialDrive'), freedom: s('freedomNeed'), relationships: s('familyOrientation'), achievement: s('achievement'),
    creativity: s('creativity'), stability: s('stabilityNeed'), adventure: s('adventure'), contribution: s('contribution'),
  }
}

export default function Simulator({ report }: { report: Report }) {
  const [p, setP] = useState<LifePriorities>(() => loadPriorities() ?? defaults(report))
  useEffect(() => savePriorities(p), [p])
  const res = useMemo(() => {
    try {
      return simulateLifestyle(p, report.scores)
    } catch {
      return null
    }
  }, [p, report.scores])

  return (
    <div>
      <PageTitle eyebrow="Ideal Life Simulator" title="Try a different mix" lead="Set how much each thing matters to you from 0 to 10 and see what kind of life that combination tends to look like.">
        <div className="mt-4"><Badge tone="warn">Exploratory. A thinking aid, not a prediction.</Badge></div>
      </PageTitle>
      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-xl font-semibold">Your priorities</h2>
            <button type="button" onClick={() => setP(defaults(report))} className="text-sm text-accent hover:underline">Reset to my answers</button>
          </div>
          <div className="space-y-5">
            {FIELDS.map((f) => (
              <div key={f.k}>
                <div className="flex justify-between"><label htmlFor={`sl-${f.k}`} className="font-medium">{f.label}</label><span className="tabular-nums text-muted">{p[f.k]}</span></div>
                <input id={`sl-${f.k}`} type="range" min={0} max={10} step={1} value={p[f.k]} onChange={(e) => setP({ ...p, [f.k]: Number(e.target.value) })} aria-describedby={`h-${f.k}`} className="w-full accent-[rgb(var(--accent))] h-6" />
                <p id={`h-${f.k}`} className="text-xs text-faint">{f.hint}</p>
              </div>
            ))}
          </div>
        </Card>
        <div className="space-y-4 lg:sticky lg:top-20 lg:self-start" aria-live="polite">
          {res ? <Result res={res} /> : <Card><p className="text-muted">The simulator could not run.</p></Card>}
        </div>
      </div>
    </div>
  )
}

function Result({ res }: { res: ReturnType<typeof simulateLifestyle> }) {
  return (
    <>
      <Card>
        <h2 className="text-2xl font-semibold tracking-tight">{res.headline}</h2>
        <p className="mt-2 text-muted leading-relaxed">{res.description}</p>
        <div className="mt-5 space-y-4">
          {res.matches.map((m) => <Bar key={m.id} label={m.name} value={m.percent} sub={m.description} tone="accent" />)}
        </div>
      </Card>
      <Card>
        <h3 className="font-semibold text-lg">Likely trade-offs</h3>
        <ul className="mt-2 list-disc pl-5 space-y-1.5 text-muted">{res.tradeoffs.map((t, i) => <li key={i}>{t}</li>)}</ul>
        {res.tensions.length > 0 && (<><h3 className="font-semibold text-lg mt-5">Tensions in this mix</h3><ul className="mt-2 list-disc pl-5 space-y-1.5 text-muted">{res.tensions.map((t, i) => <li key={i}>{t}</li>)}</ul></>)}
        <h3 className="font-semibold text-lg mt-5">A setting that could suit it</h3>
        <p className="mt-2 text-muted">{res.environment}</p>
        <p className="mt-5 text-sm text-faint">{res.caveat}</p>
      </Card>
    </>
  )
}
