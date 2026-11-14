import { QUESTIONS } from '../data/questions'
import { SECTIONS } from '../data/sections'
import { Button, Card, Eyebrow } from '../components/ui'
import { go } from '../lib/router'

export default function Intro() {
  const minutes = SECTIONS.reduce((s, x) => s + x.estMinutes, 0)
  const count = QUESTIONS.filter((q) => !q.probeFor).length
  const items: [string, string][] = [
    ['One question at a time', `There are about ${count} questions in ${SECTIONS.length} short sections. Pick the answer that fits you best. Number keys choose, Enter moves on, Backspace goes back.`],
    ['It adapts', 'Some questions only appear if they are relevant to you. At the end you may get a few follow-ups to firm up anything that is still unclear.'],
    ['Saved as you go', 'Every answer is saved on this device straight away. Close the tab and come back whenever you like.'],
    ['Be honest, not impressive', 'Answer for who you are now, not who you want to be. There are no good or bad answers, and nobody else will see them.'],
    ['Not a diagnosis', 'This is a tool for reflection. It is not a medical, clinical or career assessment, and it cannot tell you what to do.'],
  ]
  return (
    <div className="min-h-screen">
      <header className="mx-auto max-w-3xl px-5 sm:px-8 h-16 flex items-center"><a href="#/" className="text-muted hover:text-ink">&larr; Back</a></header>
      <main className="mx-auto max-w-3xl px-5 sm:px-8 pb-24 pt-6 sm:pt-12">
        <Eyebrow>Before you begin</Eyebrow>
        <h1 className="mt-2 text-4xl sm:text-6xl font-semibold tracking-tight">About {minutes} minutes, just for you.</h1>
        <p className="mt-5 text-xl text-muted">Find somewhere quiet. Take your time on the ones that make you think.</p>
        <div className="mt-10 space-y-3">
          {items.map(([t, b]) => (
            <Card key={t} className="!p-5"><h2 className="font-semibold text-lg">{t}</h2><p className="mt-1 text-muted leading-relaxed">{b}</p></Card>
          ))}
        </div>
        <div className="mt-10"><Button size="lg" onClick={() => go('/assess')}>Begin</Button></div>
      </main>
    </div>
  )
}
