import { useMemo, useState } from 'react'
import { QUESTIONS } from '../data/questions'
import { DIMENSIONS, DIM_IDS, dimLabel, dimsInGroup, isInverse, type DimGroup, type DimId } from '../engine/dimensions'
import { loadProgress } from '../engine/storage'
import type { Report } from '../engine/types'
import { Badge, Bar, Card, ConfBadge, Counter, PageTitle, ValidityBadge, cx } from '../components/ui'
import { band, describeAnswer } from '../lib/format'
import { go } from '../lib/router'

const GROUPS: [DimGroup, string][] = [
  ['bigfive', 'Personality'], ['needs', 'Basic needs'], ['ambition', 'Ambition'], ['wellbeing', 'Wellbeing'],
  ['confidence', 'Confidence'], ['relationships', 'Relationships'], ['motives', 'Enjoyment motives'], ['riasec', 'Interests'],
  ['work', 'Work'], ['values', 'Values'], ['habits', 'Habits'], ['future', 'Future self'], ['domains', 'Life domains'],
]

function meaning(d: DimId, score: number, n: number): string {
  const def = DIMENSIONS[d]
  if (n === 0) return `${def.label} belongs to ${def.framework}. None of your answers spoke to it directly, so the score sits at a neutral 50 and should not be read as a result.`
  const b = band(score)
  const place = b === 'high' ? 'in the upper part' : b === 'low' ? 'in the lower part' : 'in the middle'
  const inv = isInverse(d)
    ? ' For this one, a higher score is usually the less comfortable end, so a lower number is generally easier to live with.'
    : ''
  return `${def.label} is part of ${def.framework}. Your answers put it ${place} of the 0 to 100 range.${inv} A number like this describes how you answered today. It is not a fixed fact about you.`
}

export default function Explorer({ report, selected }: { report: Report; selected?: string }) {
  const [q, setQ] = useState('')
  const answers = useMemo(() => loadProgress()?.answers ?? {}, [])
  const qById = useMemo(() => new Map(QUESTIONS.map((x) => [x.id, x])), [])
  const dim: DimId | null = selected && selected in DIMENSIONS ? (selected as DimId) : null
  const s = dim ? report.scores[dim] : null

  const list = (g: DimGroup) => dimsInGroup(g).filter((d) => !q || dimLabel(d).toLowerCase().includes(q.toLowerCase()))

  return (
    <div>
      <PageTitle eyebrow="Discovery Explorer" title="Look behind any score" lead="Pick a dimension to see what it means, how sure we are, and exactly which of your answers shaped it." />
      <div className="grid gap-6 lg:grid-cols-[300px_1fr]">
        <aside aria-label="Dimensions" className={cx('lg:sticky lg:top-20 lg:self-start lg:max-h-[calc(100vh-6rem)] lg:overflow-y-auto', dim && 'hidden lg:block')}>
          <label htmlFor="dsearch" className="sr-only">Search dimensions</label>
          <input id="dsearch" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search" className="w-full rounded-xl border border-line bg-surface px-3.5 py-2.5 mb-4 placeholder:text-faint" />
          <div className="space-y-5">
            {GROUPS.map(([g, name]) => {
              const ids = list(g)
              if (!ids.length) return null
              return (
                <div key={g}>
                  <h2 className="text-xs font-semibold uppercase tracking-wider text-faint mb-1.5">{name}</h2>
                  <ul>
                    {ids.map((d) => (
                      <li key={d}>
                        <button type="button" onClick={() => go(`/explore/${d}`)} aria-current={d === dim ? 'true' : undefined} className={cx('w-full flex items-center justify-between rounded-lg px-2.5 py-1.5 text-left text-[15px]', d === dim ? 'bg-accent/10 text-accent' : 'hover:bg-line/50')}>
                          <span>{dimLabel(d)}</span>
                          <span className="tabular-nums text-faint">{report.scores[d].n ? Math.round(report.scores[d].score) : '–'}</span>
                        </button>
                      </li>
                    ))}
                  </ul>
                </div>
              )
            })}
          </div>
        </aside>

        <section aria-live="polite">
          {!dim || !s ? (
            <Card className="hidden lg:block text-center py-16 text-muted">Choose a dimension from the list. There are {DIM_IDS.length} of them.</Card>
          ) : (
            <div className="space-y-4">
              <button type="button" className="lg:hidden text-accent text-[15px]" onClick={() => go('/explore')}>&larr; All dimensions</button>
              <Card>
                <p className="text-sm text-faint">{DIMENSIONS[dim].framework}</p>
                <div className="mt-1 flex flex-wrap items-center justify-between gap-3">
                  <h2 className="text-3xl font-semibold tracking-tight">{dimLabel(dim)}</h2>
                  <p className="text-5xl font-semibold">{s.n ? <Counter value={s.score} /> : '–'}<span className="text-lg text-faint"> / 100</span></p>
                </div>
                <div className="mt-3 flex flex-wrap gap-2"><ConfBadge confidence={s.confidence} /><ValidityBadge dim={dim} />{isInverse(dim) && <Badge>Higher is the less comfortable end</Badge>}</div>
                <div className="mt-5"><Bar label="Score" value={s.score} /></div>
                <p className="mt-5 text-muted leading-relaxed">{meaning(dim, s.score, s.n)}</p>
                <p className="mt-3 text-sm text-faint">Confidence {Math.round(s.confidence * 100)}%, from {s.n} {s.n === 1 ? 'answer' : 'answers'}.</p>
              </Card>

              <h3 className="text-xl font-semibold pt-2">Answers behind this score</h3>
              {s.contributions.length === 0 ? (
                <Card><p className="text-muted">No answers fed this dimension.</p></Card>
              ) : (
                <ul className="space-y-2.5">
                  {[...s.contributions].sort((a, b) => b.weight - a.weight).map((c, i) => {
                    const qq = qById.get(c.questionId)
                    const a = answers[c.questionId]
                    return (
                      <li key={c.questionId + i} className="rounded-2xl border border-line bg-surface p-4">
                        <p className="font-medium leading-snug">{qq?.text ?? c.questionId}</p>
                        <p className="mt-1.5 text-muted">{a === undefined ? 'Your answer is not stored on this device.' : <>You answered: <span className="text-ink">{describeAnswer(qq, a)}</span></>}</p>
                        <div className="mt-2.5 flex items-center gap-3 text-xs text-faint">
                          <span>Pointed to {Math.round(c.level * 100)} of 100</span>
                          <span className="h-1.5 flex-1 max-w-[180px] rounded-full bg-line/70 overflow-hidden"><span className="block h-full bg-accent" style={{ width: `${c.level * 100}%` }} /></span>
                          <span>Weight {c.weight.toFixed(2)}</span>
                        </div>
                      </li>
                    )
                  })}
                </ul>
              )}
            </div>
          )}
        </section>
      </div>
    </div>
  )
}
