import { useMemo, useState } from 'react'
import { deleteSnapshot, loadHistory } from '../engine/storage'
import { Lines } from '../components/Charts'
import { Button, Card, PageTitle, cx } from '../components/ui'
import { dimLabel } from '../engine/dimensions'
import { go } from '../lib/router'

const BIG5 = ['openness', 'conscientiousness', 'extraversion', 'agreeableness', 'emotionalStability'] as const
const COLORS = ['#0071e3', '#248a3d', '#c93400', '#8e44ad', '#b7791f', '#0e8a8a', '#7f8c8d', '#c2185b']

export default function Growth() {
  const [hist, setHist] = useState(loadHistory())
  const keys = [
    { key: 'happiness', name: 'Happiness' }, { key: 'alignment', name: 'Career alignment' }, { key: 'confidence', name: 'Confidence in results' },
    ...BIG5.map((b) => ({ key: b, name: dimLabel(b) })),
  ]
  const [on, setOn] = useState<string[]>(['happiness', 'alignment', 'confidence'])
  const data = useMemo(
    () => hist.map((s) => ({
      date: new Date(s.takenAt).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: hist.length > 6 ? '2-digit' : undefined }),
      happiness: Math.round(s.happiness), alignment: Math.round(s.careerAlignment), confidence: Math.round(s.overallConfidence * 100),
      ...Object.fromEntries(BIG5.map((b) => [b, Math.round(s.scores[b] ?? 0)])),
    })),
    [hist],
  )
  const series = keys.map((k, i) => ({ ...k, color: COLORS[i % COLORS.length] })).filter((k) => on.includes(k.key))

  return (
    <div>
      <PageTitle eyebrow="Growth tracker" title="How you change over time" lead="Each time you finish the questionnaire, a snapshot of your scores is saved here. No answers are kept in the snapshot, only the scores." />
      {hist.length === 0 ? (
        <Card className="py-12 text-center"><h2 className="text-2xl font-semibold">No snapshots yet</h2><p className="mt-2 text-muted">Finish the questionnaire to save your first one.</p></Card>
      ) : (
        <>
          <Card>
            <div className="flex flex-wrap gap-2 mb-4" role="group" aria-label="Lines to show">
              {keys.map((k, i) => {
                const sel = on.includes(k.key)
                return (
                  <button key={k.key} type="button" aria-pressed={sel} onClick={() => setOn(sel ? on.filter((x) => x !== k.key) : [...on, k.key])} className={cx('inline-flex items-center gap-2 rounded-full border px-3 py-1 text-sm', sel ? 'border-ink/40 bg-raised' : 'border-line text-faint')}>
                    <i className="h-2.5 w-2.5 rounded-full" style={{ background: COLORS[i % COLORS.length], opacity: sel ? 1 : 0.35 }} />{k.name}
                  </button>
                )
              })}
            </div>
            <Lines data={data} series={series} />
            {hist.length < 2 && <p className="mt-3 text-sm text-muted">You have one snapshot so far. Retake the questionnaire another time to see a line form.</p>}
          </Card>
          <p className="mt-4 rounded-xl border border-line bg-raised p-4 text-sm text-muted">Changes between snapshots are not necessarily lasting changes in your personality. Mood, a rough week, how you read the questions and plain noise all move scores around. Look for slow, steady drift rather than single jumps.</p>

          <h2 className="mt-10 mb-3 text-xl font-semibold">Snapshots</h2>
          <ul className="space-y-2.5">
            {[...hist].reverse().map((s) => (
              <li key={s.id} className="rounded-2xl border border-line bg-surface p-4 flex flex-wrap items-center justify-between gap-3">
                <div>
                  <p className="font-medium">{new Date(s.takenAt).toLocaleString(undefined, { dateStyle: 'long', timeStyle: 'short' })}</p>
                  <p className="text-sm text-muted">Happiness {Math.round(s.happiness)}, alignment {Math.round(s.careerAlignment)}, confidence {Math.round(s.overallConfidence * 100)}%</p>
                  <p className="text-sm text-faint">Top values: {s.topValues.join(', ')}</p>
                </div>
                <Button variant="secondary" onClick={() => { if (window.confirm('Delete this snapshot?')) { deleteSnapshot(s.id); setHist(loadHistory()) } }} aria-label={`Delete snapshot from ${new Date(s.takenAt).toLocaleDateString()}`}>Delete</Button>
              </li>
            ))}
          </ul>
        </>
      )}
      <div className="mt-8"><Button variant="secondary" onClick={() => go('/intro')}>Retake the questionnaire</Button></div>
    </div>
  )
}
