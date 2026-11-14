import { useMemo } from 'react'
import { DIMENSIONS, dimLabel, dimsInGroup, isInverse, isValidated, type DimId } from '../engine/dimensions'
import type { Report } from '../engine/types'
import { GroupedBars, HBars, RadarBlock, type Pt } from '../components/Charts'
import { HumanViz } from '../components/HumanViz'
import { useColors } from '../components/useColors'
import { Badge, Bar, Button, Card, ConfBadge, Counter, Eyebrow, InsightGrid, Paras, Reveal, SectionHead, SubHead, ValidityBadge, cx } from '../components/ui'
import { confLabel } from '../lib/format'
import { go } from '../lib/router'

const SECTIONS_NAV: [string, string, string][] = [
  ['a', 'A', 'Identity'],
  ['b', 'B', 'Psychology'],
  ['c', 'C', 'Ambitions'],
  ['d', 'D', 'Happiness'],
  ['e', 'E', 'Relationships'],
  ['f', 'F', 'Career'],
  ['g', 'G', 'Hidden potential'],
  ['h', 'H', 'Roadmap'],
  ['i', 'I', 'Summary'],
]

const BIG5: DimId[] = ['openness', 'conscientiousness', 'extraversion', 'agreeableness', 'emotionalStability']
const FACETS: DimId[] = ['riskTolerance', 'impulsivity', 'adaptability', 'structurePreference', 'assertiveness', 'curiosity', 'creativity']

function jump(id: string) {
  document.getElementById(`sec-${id}`)?.scrollIntoView({ behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth', block: 'start' })
}

export default function Results({ report: r }: { report: Report }) {
  const c = useColors()
  const date = new Date(r.generatedAt).toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' })

  const { strengths, growth } = useMemo(() => {
    const groups = ['bigfive', 'needs', 'confidence', 'relationships', 'habits', 'wellbeing'] as const
    const all = groups.flatMap((g) => dimsInGroup(g)).filter((d) => r.scores[d].n > 0 && d !== 'recentMood')
    const sure = all.filter((d) => r.scores[d].confidence >= 0.3)
    const pool = (sure.length >= 12 ? sure : all).map((d) => ({
      label: isInverse(d) ? `Low ${dimLabel(d).toLowerCase()}` : dimLabel(d),
      value: isInverse(d) ? 100 - r.scores[d].score : r.scores[d].score,
    }))
    pool.sort((a, b) => b.value - a.value)
    return { strengths: pool.slice(0, 6), growth: pool.slice(-6).reverse() }
  }, [r])
  const sg: Pt[] = [...strengths, ...growth]

  return (
    <div>
      {/* Hero visual */}
      <section className="text-center" aria-labelledby="res-title">
        <Eyebrow>Your results, {date}</Eyebrow>
        <h1 id="res-title" className="mt-2 text-4xl sm:text-6xl font-semibold tracking-tight">Your Human Index</h1>
        <p className="mt-4 mx-auto max-w-2xl text-xl sm:text-2xl text-muted leading-snug">{r.identity.headline}</p>
        <div className="mt-6 flex flex-wrap items-center justify-center gap-3">
          <span className="text-sm text-muted">Overall confidence</span>
          <span className="text-2xl font-semibold"><Counter value={r.quality.overallConfidence * 100} suffix="%" /></span>
          <ConfBadge confidence={r.quality.overallConfidence} />
        </div>
        <div className="no-print mt-5 flex flex-wrap justify-center gap-2">
          <Button onClick={() => window.print()}>Save as PDF</Button>
          <Button variant="secondary" onClick={() => go('/share')}>Share a simple version</Button>
        </div>
        <div className="mt-8"><HumanViz report={r} /></div>
      </section>

      {/* Section nav */}
      <nav aria-label="Report sections" className="no-print sticky top-14 z-20 -mx-4 sm:mx-0 mt-12 mb-10 border-y sm:border sm:rounded-full border-line bg-bg/85 backdrop-blur-md">
        <ul className="flex gap-1 overflow-x-auto px-3 py-2 [scrollbar-width:none] sm:justify-center">
          {SECTIONS_NAV.map(([id, letter, name]) => (
            <li key={id}>
              <button type="button" onClick={() => jump(id)} className="rounded-full px-3 py-1 text-sm whitespace-nowrap text-muted hover:text-ink hover:bg-line/50">
                <span className="text-faint mr-1">{letter}</span>{name}
              </button>
            </li>
          ))}
        </ul>
      </nav>

      <div className="space-y-20">
        {/* A */}
        <section aria-labelledby="h-a">
          <SectionHead id="sec-a" letter="A" title="Identity" lead="Who you seem to be, based on how you answered." />
          <Reveal><Card><Paras items={r.identity.overview} /></Card></Reveal>
          <SubHead>Core traits</SubHead>
          <InsightGrid items={r.identity.coreTraits} />
          <div className="mt-6 grid gap-3 md:grid-cols-3">
            {([['How you tend to act', r.identity.behavioral], ['How you are with people', r.identity.social], ['How you decide', r.identity.decisionStyle]] as const).map(([t, b]) => (
              <Reveal key={t}><Card className="h-full"><h4 className="font-semibold text-lg">{t}</h4><p className="mt-2 text-muted leading-relaxed">{b}</p></Card></Reveal>
            ))}
          </div>
          <SubHead>Strengths</SubHead>
          <InsightGrid items={r.identity.strengths} />
          <SubHead note="Worded as possibilities. You know yourself better than a questionnaire does.">Possible blind spots</SubHead>
          <InsightGrid items={r.identity.blindSpots} />
        </section>

        {/* B */}
        <section className="print-break">
          <SectionHead id="sec-b" letter="B" title="Psychological profile" lead="Five broad personality traits, plus some finer tendencies." />
          <div className="grid gap-4 lg:grid-cols-2">
            <Reveal>
              <Card className="h-full">
                <h3 className="text-xl font-semibold">Big Five</h3>
                <p className="text-sm text-faint">Measured with the Mini-IPIP, a short published questionnaire.</p>
                <div className="mt-5 space-y-5">
                  {BIG5.map((d) => (
                    <Bar key={d} label={dimLabel(d)} value={r.scores[d].score} right={<ConfBadge confidence={r.scores[d].confidence} />} />
                  ))}
                </div>
                <div className="mt-4 flex flex-wrap gap-2"><ValidityBadge dim="openness" /><Badge>Scores are relative to the scale, not to other people</Badge></div>
              </Card>
            </Reveal>
            <Reveal delay={0.08}>
              <Card className="h-full">
                <h3 className="text-xl font-semibold">Shape of your profile</h3>
                <RadarBlock data={BIG5.map((d) => ({ label: dimLabel(d), value: r.scores[d].score }))} height={290} />
              </Card>
            </Reveal>
          </div>

          <SubHead note="Custom scores written for this tool. Treat them as prompts for thought.">Finer tendencies</SubHead>
          <Reveal>
            <Card>
              <div className="grid gap-x-10 gap-y-5 md:grid-cols-2">
                {FACETS.map((d) => (
                  <Bar key={d} label={dimLabel(d)} value={r.scores[d].score} right={<><Badge>Exploratory</Badge><ConfBadge confidence={r.scores[d].confidence} /></>} />
                ))}
              </div>
            </Card>
          </Reveal>

          <SubHead>Type sketch</SubHead>
          <Reveal>
            <Card>
              <div className="flex flex-wrap items-center gap-4">
                <span className="text-5xl font-semibold tracking-[0.2em]">{r.typeSketch.code}</span>
                <Badge tone="warn">Not a validated measure</Badge>
              </div>
              <Paras className="mt-4" items={[r.typeSketch.description]} />
              <div className="mt-5 grid gap-4 sm:grid-cols-2">
                {r.typeSketch.axes.map((a) => (
                  <Bar key={a.axis} label={`${a.axis}: ${a.letter}`} value={50 + a.strength * 50} sub={a.strength < 0.2 ? 'Close to the middle, so this letter could easily flip.' : undefined} />
                ))}
              </div>
              <p className="mt-5 rounded-xl bg-warn/10 p-3 text-sm text-ink">{r.typeSketch.caveat}</p>
            </Card>
          </Reveal>

          <SubHead>Confidence and self-perception</SubHead>
          <Paras items={r.confidence.overview} />
          <div className="mt-4 grid gap-4 lg:grid-cols-2">
            <Reveal><Card><HBars data={r.confidence.profile.map((p) => ({ label: p.label, value: p.score }))} /></Card></Reveal>
            <InsightGrid items={r.confidence.hypotheses} cols={1} />
          </div>

          <SubHead>Habits and follow-through</SubHead>
          <Paras items={r.habits.overview} />
          <div className="mt-4 grid gap-4 lg:grid-cols-2">
            <Reveal><Card><HBars data={r.habits.profile.map((p) => ({ label: p.label, value: p.score }))} /></Card></Reveal>
            <Reveal delay={0.06}>
              <Card className="h-full">
                <Badge tone="accent">Main sticking point: {{ motivation: 'motivation', clarity: 'clarity', environment: 'environment', priorities: 'competing priorities', none: 'none stands out' }[r.habits.bottleneck.kind]}</Badge>
                <p className="mt-3 text-muted leading-relaxed">{r.habits.bottleneck.reading}</p>
              </Card>
            </Reveal>
          </div>
        </section>

        {/* C */}
        <section>
          <SectionHead id="sec-c" letter="C" title="Ambitions" lead="What drives you, and where your drive might be pulling in different directions." />
          <Paras items={r.ambitions.overview} />
          <div className="mt-6 grid gap-4 lg:grid-cols-2">
            <Reveal>
              <Card className="h-full">
                <h3 className="text-xl font-semibold">Strongest drivers</h3>
                <HBars data={r.ambitions.drivers.slice(0, 8).map((d) => ({ label: d.label, value: d.score }))} />
              </Card>
            </Reveal>
            <Reveal delay={0.06}>
              <Card className="h-full">
                <h3 className="text-xl font-semibold">From inside or outside</h3>
                <div className="mt-4 space-y-4">
                  <Bar label="Intrinsic: doing it for its own sake" value={r.ambitions.intrinsicVsExtrinsic.intrinsic} tone="good" />
                  <Bar label="Extrinsic: doing it for the reward" value={r.ambitions.intrinsicVsExtrinsic.extrinsic} />
                </div>
                <p className="mt-4 text-muted leading-relaxed">{r.ambitions.intrinsicVsExtrinsic.reading}</p>
              </Card>
            </Reveal>
          </div>
          <details className="mt-4 rounded-2xl border border-line bg-surface p-4">
            <summary className="cursor-pointer font-medium">Why these drivers?</summary>
            <ul className="mt-3 space-y-2 text-muted">{r.ambitions.drivers.slice(0, 8).map((d) => <li key={d.dim}><span className="text-ink font-medium">{d.label}.</span> {d.why}</li>)}</ul>
          </details>
          <SubHead note="Gaps between what you say, what your choices suggest, and what is expected of you.">Stated and revealed</SubHead>
          <InsightGrid items={r.ambitions.statedVsRevealed} />
          {r.ambitions.conflicts.length > 0 && (<><SubHead>Where your ambitions pull apart</SubHead><InsightGrid items={r.ambitions.conflicts} /></>)}
          {r.ambitions.overlooked.length > 0 && (<><SubHead>Easy to overlook</SubHead><InsightGrid items={r.ambitions.overlooked} /></>)}

          <SubHead>What you value</SubHead>
          <Paras items={[r.values.overview]} />
          <div className="mt-4 grid gap-4 lg:grid-cols-2">
            <Reveal><Card><h3 className="text-lg font-semibold mb-1">Values ranking</h3><HBars data={r.values.ranked.slice(0, 10).map((v) => ({ label: v.label, value: v.score }))} /></Card></Reveal>
            <Reveal delay={0.06}><Card><h3 className="text-lg font-semibold mb-1">Why activities feel good</h3><p className="text-sm text-faint">Your enjoyment motives.</p><HBars data={r.motives.map((m) => ({ label: m.label, value: m.score })).sort((a, b) => b.value - a.value).slice(0, 10)} /></Card></Reveal>
          </div>
          {r.values.conflicts.length > 0 && <div className="mt-4"><InsightGrid items={r.values.conflicts} /></div>}
        </section>

        {/* D */}
        <section className="print-break">
          <SectionHead id="sec-d" letter="D" title="Happiness" lead="How life feels right now, and what seems to lift or weigh on it." />
          <div className="grid gap-4 lg:grid-cols-5">
            <Reveal className="lg:col-span-2">
              <Card className="h-full text-center flex flex-col items-center justify-center">
                <p className="text-sm text-muted">Overall wellbeing</p>
                <p className="text-7xl font-semibold tracking-tight"><Counter value={r.happiness.overall} /></p>
                <p className="text-sm text-faint">out of 100</p>
                <div className="mt-3 flex flex-wrap justify-center gap-2">
                  <ConfBadge confidence={r.scores.lifeSatisfaction.confidence} />
                  <ValidityBadge dim="lifeSatisfaction" />
                </div>
                <p className="mt-3 text-xs text-faint max-w-xs">A composite of life satisfaction (a published scale) and wellbeing measures that are exploratory.</p>
              </Card>
            </Reveal>
            <Reveal className="lg:col-span-3" delay={0.06}>
              <Card className="h-full"><h3 className="text-lg font-semibold">Five parts of wellbeing</h3><RadarBlock data={r.happiness.perma.map((p) => ({ label: p.label, value: p.score }))} height={260} /></Card>
            </Reveal>
          </div>
          <Paras className="mt-6" items={r.happiness.overview} />
          <SubHead>What seems to lift you</SubHead>
          <InsightGrid items={r.happiness.lifts} />
          <SubHead>What seems to weigh on you</SubHead>
          <InsightGrid items={r.happiness.drags} />
          <Reveal><Card className="mt-6"><h4 className="font-semibold text-lg">A mood or a trait?</h4><p className="mt-2 text-muted leading-relaxed">{r.happiness.stateVsTrait}</p></Card></Reveal>
        </section>

        {/* E */}
        <section>
          <SectionHead id="sec-e" letter="E" title="Relationships" lead="How you connect, and what you need from the people around you." />
          <Paras items={r.relationships.overview} />
          <Reveal><Card className="mt-6"><HBars data={r.relationships.profile.map((p) => ({ label: p.label, value: p.score }))} /></Card></Reveal>
          <SubHead>Strengths</SubHead><InsightGrid items={r.relationships.strengths} />
          <SubHead>What you need</SubHead><InsightGrid items={r.relationships.needs} />
          <SubHead>Where to grow</SubHead><InsightGrid items={r.relationships.growth} />
          <Reveal><Card className="mt-6"><h4 className="font-semibold text-lg">Now compared with ideal</h4><p className="mt-2 text-muted leading-relaxed">{r.relationships.currentVsIdeal}</p></Card></Reveal>
        </section>

        {/* F */}
        <section className="print-break">
          <SectionHead id="sec-f" letter="F" title="Career alignment" lead="How well your work direction seems to fit you. This is exploratory." />
          <div className="grid gap-4 lg:grid-cols-5">
            <Reveal className="lg:col-span-2">
              <Card className="h-full text-center flex flex-col items-center justify-center">
                <p className="text-sm text-muted">Alignment</p>
                <p className="text-7xl font-semibold tracking-tight"><Counter value={r.career.alignmentScore} /></p>
                <div className="mt-3 flex flex-wrap justify-center gap-2"><Badge>Exploratory</Badge><Badge tone="accent">Interest code {r.career.riasecCode}</Badge></div>
              </Card>
            </Reveal>
            <Reveal className="lg:col-span-3" delay={0.06}>
              <Card className="h-full"><h3 className="text-lg font-semibold">What makes up the score</h3><HBars data={r.career.components.map((x) => ({ label: x.label, value: x.score }))} /></Card>
            </Reveal>
          </div>
          <div className="mt-4 grid gap-3 md:grid-cols-2">
            {r.career.components.map((x) => (
              <div key={x.label} className="rounded-2xl border border-line bg-raised p-4"><p className="font-medium">{x.label} <span className="text-faint tabular-nums">{Math.round(x.score)}</span></p><p className="mt-1 text-sm text-muted">{x.explanation}</p></div>
            ))}
          </div>
          <SubHead note="Holland's six interest types.">Interest profile</SubHead>
          <div className="grid gap-4 lg:grid-cols-2">
            <Reveal><Card><RadarBlock data={r.career.riasec.map((p) => ({ label: p.label, value: p.score }))} height={280} /></Card></Reveal>
            <Reveal delay={0.06}><Card className="h-full"><h4 className="font-semibold text-lg">Work environment that may suit you</h4><p className="mt-2 text-muted leading-relaxed">{r.career.environment}</p></Card></Reveal>
          </div>
          <Paras className="mt-6" items={r.career.overview} />
          {r.career.conflicts.length > 0 && (<><SubHead>Tensions to look at</SubHead><InsightGrid items={r.career.conflicts} /></>)}
          <SubHead note="Ideas to explore, not recommendations to act on.">Directions worth a look</SubHead>
          <div className="grid gap-3 md:grid-cols-2">
            {r.career.alternatives.map((a) => (
              <Reveal key={a.id}>
                <Card className="h-full">
                  <div className="flex items-start justify-between gap-3"><h4 className="font-semibold text-lg">{a.title}</h4><span className="text-2xl font-semibold tabular-nums">{Math.round(a.matchScore)}</span></div>
                  <p className="text-xs text-faint mt-0.5">Interest code {a.riasec}</p>
                  <p className="mt-2 text-muted">{a.why}</p>
                  <p className="mt-2 text-sm text-faint"><span className="font-medium text-muted">Trade-offs.</span> {a.tradeoffs}</p>
                </Card>
              </Reveal>
            ))}
          </div>
        </section>

        {/* G */}
        <section>
          <SectionHead id="sec-g" letter="G" title="Hidden potential" lead="Things that may be there, waiting for a reason to show up." />
          <InsightGrid items={r.hiddenPotential} />
          <SubHead note="How each area of life is going now, against how much it matters to you.">Now and ideal</SubHead>
          <Reveal><Card><GroupedBars data={r.future.gaps.map((g) => ({ label: g.domain, a: g.current, b: g.ideal }))} a="Now" b="Ideal" /></Card></Reveal>
          <Paras className="mt-5" items={r.future.overview} />
          <InsightGrid items={r.future.trajectory} />
          <SubHead note="Your highest and lowest scores among the traits that are easiest to change.">Strengths and growth areas</SubHead>
          <Reveal>
            <Card>
              <HBars data={sg} colorFn={(_, i) => (i < strengths.length ? c.good : c.warn)} />
              <div className="mt-2 flex gap-4 text-sm text-muted"><span className="inline-flex items-center gap-1.5"><i className="h-2.5 w-2.5 rounded-full bg-good" /> Strengths</span><span className="inline-flex items-center gap-1.5"><i className="h-2.5 w-2.5 rounded-full bg-warn" /> Room to grow</span></div>
            </Card>
          </Reveal>
        </section>

        {/* H */}
        <section className="print-break">
          <SectionHead id="sec-h" letter="H" title="Growth roadmap" lead="Small, concrete moves, grouped by how far ahead they look." />
          {([['Next few weeks', r.roadmap.short], ['Next few months', r.roadmap.medium], ['Next few years', r.roadmap.long]] as const).map(([t, items]) => (
            <div key={t}><SubHead>{t}</SubHead><InsightGrid items={items} cols={3} /></div>
          ))}
          <div className="mt-8 grid gap-4 md:grid-cols-2">
            {([['Experiences to try', r.roadmap.experiences], ['Habits to build', r.roadmap.habits], ['Questions to reflect on', r.roadmap.reflections], ['Skills to develop', r.roadmap.skills]] as const).map(([t, items]) => (
              <Reveal key={t}><Card className="h-full"><h4 className="font-semibold text-lg">{t}</h4><ul className="mt-3 space-y-2 text-muted list-disc pl-5">{items.map((x, i) => <li key={i}>{x}</li>)}</ul></Card></Reveal>
            ))}
          </div>
        </section>

        {/* I */}
        <section>
          <SectionHead id="sec-i" letter="I" title="Personal summary" />
          <Reveal><Card><Paras items={r.summary} className="text-[19px]" /></Card></Reveal>

          <SubHead note="Where your results sit against published reference data, when there is any worth using.">Population comparison</SubHead>
          {r.population.length === 0 ? (
            <Card><p className="text-muted leading-relaxed">There is no reliable comparison data for these results, so none is shown. Most of the scores here are custom, and putting a percentile on them would only look more precise than it is.</p></Card>
          ) : (
            <div className="grid gap-3 md:grid-cols-2">
              {r.population.map((p) => (
                <Card key={p.id}>
                  <h4 className="font-semibold text-lg">{p.label}</h4>
                  <p className="mt-1"><span className="text-2xl font-semibold">{p.userValue}</span> <span className="text-muted">{p.band}</span></p>
                  <dl className="mt-3 space-y-1.5 text-sm text-muted">
                    {([['Source', p.source], ['Sample', p.sample], ['Population', p.population], ['Instrument', p.instrument], ['Limitations', p.limitations]] as const).map(([k, v]) => (
                      <div key={k}><dt className="inline font-medium text-ink">{k}. </dt><dd className="inline">{v}</dd></div>
                    ))}
                  </dl>
                </Card>
              ))}
            </div>
          )}

          <SubHead note="How much weight to put on all of this.">How reliable are these results</SubHead>
          <Card>
            <div className="flex flex-wrap items-center gap-3">
              <span className="text-3xl font-semibold"><Counter value={r.quality.overallConfidence * 100} suffix="%" /></span>
              <ConfBadge confidence={r.quality.overallConfidence} />
              <span className="text-sm text-muted">{r.quality.answered} of {r.quality.shown} questions answered. {confLabel(r.quality.overallConfidence)} overall.</span>
            </div>
            {r.quality.flags.length > 0 ? (
              <ul className="mt-4 space-y-1.5 text-sm text-muted">{r.quality.flags.map((f) => <li key={f.id}><Badge tone="warn">{f.id.replace('-', ' ')}</Badge> <span className="ml-1">{f.detail}</span></li>)}</ul>
            ) : <p className="mt-3 text-sm text-muted">No response-quality concerns were noticed.</p>}
            <p className="mt-4 text-sm text-faint">{Object.keys(DIMENSIONS).filter((d) => isValidated(d as DimId)).length} scores come from published instruments; everything else is exploratory. This is a reflection tool. It is not a diagnosis or a clinical assessment.</p>
          </Card>
        </section>
      </div>

      <div className={cx('no-print mt-16 flex flex-wrap gap-2 justify-center')}>
        <Button variant="secondary" onClick={() => go('/explore')}>Open the Explorer</Button>
        <Button variant="secondary" onClick={() => go('/try')}>What should I try</Button>
        <Button variant="secondary" onClick={() => go('/growth')}>Growth tracker</Button>
      </div>
    </div>
  )
}
