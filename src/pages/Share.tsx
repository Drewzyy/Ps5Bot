import { useState } from 'react'
import type { Report } from '../engine/types'
import { dimLabel } from '../engine/dimensions'
import { Badge, Bar, Button, Card, PageTitle } from '../components/ui'

const BIG5 = ['openness', 'conscientiousness', 'extraversion', 'agreeableness', 'emotionalStability'] as const

export default function Share({ report: r }: { report: Report }) {
  const [copied, setCopied] = useState(false)
  const values = r.values.ranked.slice(0, 5)
  const hobbies = r.hobbies.slice(0, 3)
  const text = [
    'My Human Index (simplified)',
    '',
    r.identity.headline,
    '',
    `Type sketch: ${r.typeSketch.code} (not a validated measure)`,
    `Top values: ${values.map((v) => v.label).join(', ')}`,
    '',
    'Big Five (0 to 100):',
    ...BIG5.map((b) => `- ${dimLabel(b)}: ${Math.round(r.scores[b].score)}`),
    '',
    `Things I might try: ${hobbies.map((h) => h.name).join(', ')}`,
    '',
    'From a self-report reflection tool. Not a diagnosis.',
  ].join('\n')

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text)
    } catch {
      const ta = document.createElement('textarea')
      ta.value = text
      document.body.appendChild(ta)
      ta.select()
      document.execCommand('copy')
      ta.remove()
    }
    setCopied(true)
    window.setTimeout(() => setCopied(false), 2200)
  }

  return (
    <div>
      <div className="no-print">
        <PageTitle eyebrow="Share" title="A simplified profile" lead="This view leaves out all of your answers and the detailed scores, so it is safer to show to someone else.">
          <div className="mt-5 flex flex-wrap gap-2">
            <Button onClick={copy}>{copied ? 'Copied' : 'Copy as text'}</Button>
            <Button variant="secondary" onClick={() => window.print()}>Print or save as PDF</Button>
          </div>
          <p className="sr-only" aria-live="polite">{copied ? 'Copied to clipboard' : ''}</p>
        </PageTitle>
      </div>
      <Card className="max-w-2xl mx-auto">
        <p className="text-sm font-semibold uppercase tracking-wider text-accent">The Human Index</p>
        <h2 className="mt-2 text-3xl font-semibold tracking-tight leading-tight">{r.identity.headline}</h2>
        <div className="mt-6 flex flex-wrap items-center gap-3"><span className="text-4xl font-semibold tracking-[0.18em]">{r.typeSketch.code}</span><Badge tone="warn">Type sketch, not a validated measure</Badge></div>
        <h3 className="mt-8 font-semibold text-lg">Top values</h3>
        <p className="mt-1 text-muted">{values.map((v) => v.label).join(', ')}</p>
        <h3 className="mt-8 font-semibold text-lg">Big Five</h3>
        <div className="mt-3 space-y-4">{BIG5.map((b) => <Bar key={b} label={dimLabel(b)} value={r.scores[b].score} />)}</div>
        <h3 className="mt-8 font-semibold text-lg">Things I might try</h3>
        <ul className="mt-1 text-muted list-disc pl-5">{hobbies.map((h) => <li key={h.id}>{h.name}</li>)}</ul>
        <p className="mt-8 text-xs text-faint">From a self-report reflection tool. Not a diagnosis.</p>
      </Card>
      <details className="no-print max-w-2xl mx-auto mt-6 rounded-2xl border border-line bg-surface p-4">
        <summary className="cursor-pointer font-medium">Plain text version</summary>
        <pre className="mt-3 whitespace-pre-wrap text-sm text-muted font-sans">{text}</pre>
      </details>
    </div>
  )
}
