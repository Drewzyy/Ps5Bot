import type { Report } from '../engine/types'
import { Badge, Card, PageTitle, Reveal } from '../components/ui'
import { go } from '../lib/router'

export default function Contradictions({ report }: { report: Report }) {
  const items = [...report.contradictions].sort((a, b) => b.strength - a.strength)
  return (
    <div>
      <PageTitle eyebrow="Contradiction Explorer" title="Where you pull in two directions" lead="Most people want things that do not fit neatly together. These are tensions in your answers, offered as questions to think about rather than problems to fix." />
      {items.length === 0 ? (
        <Card className="py-12 text-center"><h2 className="text-2xl font-semibold">Nothing stands out</h2><p className="mt-2 text-muted max-w-md mx-auto">Your answers did not show any strong tensions. That can also mean your answers were close to the middle on most things.</p></Card>
      ) : (
        <div className="space-y-4">
          {items.map((c) => (
            <Reveal key={c.id}>
              <Card as="article">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <h2 className="text-2xl font-semibold tracking-tight max-w-xl">{c.title}</h2>
                  <Badge tone={c.strength > 0.66 ? 'warn' : 'neutral'}>{c.strength > 0.66 ? 'Strong tension' : c.strength > 0.33 ? 'Moderate tension' : 'Mild tension'}</Badge>
                </div>
                <div className="mt-5 grid gap-3 sm:grid-cols-[1fr_auto_1fr] items-stretch">
                  {[c.a, c.b].map((side, i) => (
                    <div key={i} className={i === 1 ? 'sm:order-3' : ''}>
                      <button type="button" onClick={() => go(`/explore/${side.dim}`)} className="h-full w-full rounded-2xl border border-line bg-raised p-4 text-left hover:border-accent">
                        <span className="block text-xs text-faint">{i === 0 ? 'On one hand' : 'On the other'}</span>
                        <span className="block mt-1 font-medium">{side.label}</span>
                      </button>
                    </div>
                  ))}
                  <div className="sm:order-2 flex items-center justify-center text-faint text-sm" aria-hidden>versus</div>
                </div>
                <div className="mt-3 h-1.5 rounded-full bg-line/70 overflow-hidden" role="meter" aria-label="Strength of tension" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(c.strength * 100)}><div className="h-full bg-warn" style={{ width: `${c.strength * 100}%` }} /></div>
                <p className="mt-4 text-muted leading-relaxed">{c.explanation}</p>
                <p className="mt-4 rounded-xl bg-accent/10 p-4 text-ink"><span className="block text-xs font-semibold uppercase tracking-wider text-accent mb-1">Something to sit with</span>{c.question}</p>
              </Card>
            </Reveal>
          ))}
        </div>
      )}
    </div>
  )
}
