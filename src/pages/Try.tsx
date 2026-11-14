import type { Report } from '../engine/types'
import { dimLabel } from '../engine/dimensions'
import { Badge, Card, Counter, PageTitle, Reveal } from '../components/ui'

export default function Try({ report }: { report: Report }) {
  return (
    <div>
      <PageTitle eyebrow="What should I try" title="Things you might love" lead="Matched to why activities feel good to you, not to what is popular. The discovery score shows how well each one lines up with your motives." />
      <div className="grid gap-4 md:grid-cols-2">
        {report.hobbies.map((h, i) => (
          <Reveal key={h.id} delay={(i % 2) * 0.05}>
            <Card as="article" className="h-full flex flex-col">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <h2 className="text-2xl font-semibold tracking-tight">{h.name}</h2>
                  <div className="mt-2 flex flex-wrap gap-1.5">
                    <Badge tone={h.difficulty === 'Easy' ? 'good' : h.difficulty === 'Demanding' ? 'warn' : 'neutral'}>{h.difficulty}</Badge>
                    <Badge>{h.cost}</Badge>
                    {h.alreadyDoesSimilar && <Badge tone="accent">You already do something similar</Badge>}
                  </div>
                </div>
                <div className="text-right shrink-0">
                  <p className="text-4xl font-semibold leading-none"><Counter value={h.matchScore} /></p>
                  <p className="text-xs text-faint mt-1">discovery score</p>
                </div>
              </div>
              <p className="mt-4 text-muted leading-relaxed">{h.why}</p>
              <dl className="mt-4 space-y-2.5 text-[15px]">
                <div><dt className="font-medium">You might enjoy</dt><dd className="text-muted">{h.mightEnjoy}</dd></div>
                <div><dt className="font-medium">You might not enjoy</dt><dd className="text-muted">{h.mightDislike}</dd></div>
                <div><dt className="font-medium">A first step</dt><dd className="text-muted">{h.firstStep}</dd></div>
              </dl>
              <p className="mt-auto pt-4 text-xs text-faint">Driven by: {h.motives.map(dimLabel).join(', ')}</p>
            </Card>
          </Reveal>
        ))}
      </div>
    </div>
  )
}
