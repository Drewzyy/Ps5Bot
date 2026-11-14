import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { QUESTIONS } from '../data/questions'
import { SECTIONS } from '../data/sections'
import { isAnswered, selectProbes, visibleQuestions } from '../engine/scoring'
import { buildReport } from '../engine/report'
import { addSnapshot, loadProgress, saveProgress, saveReport } from '../engine/storage'
import type { Answers, AnswerValue, ModuleId, Progress, Question, Report } from '../engine/types'
import { QuestionInput } from '../components/QuestionInput'
import { Button, Eyebrow } from '../components/ui'
import { go } from '../lib/router'

type Phase = { kind: 'question' } | { kind: 'section'; module: ModuleId } | { kind: 'probes' } | { kind: 'building' }

interface St {
  answers: Answers
  timings: Record<string, number>
  probeIds: string[]
  currentId: string | null
}

function init(): { st: St; startedAt: string; resumed: boolean } {
  const p = loadProgress()
  if (p && Object.keys(p.answers).length > 0) {
    const vis = visibleQuestions(QUESTIONS, p.answers, p.probeIds ?? [])
    const cur = vis.find((q) => q.id === p.currentId) ?? vis.find((q) => !isAnswered(p.answers[q.id])) ?? vis[vis.length - 1]
    return { st: { answers: p.answers, timings: p.timings ?? {}, probeIds: p.probeIds ?? [], currentId: cur?.id ?? null }, startedAt: p.startedAt, resumed: true }
  }
  const first = visibleQuestions(QUESTIONS, {}, [])[0]
  return { st: { answers: {}, timings: {}, probeIds: [], currentId: first?.id ?? null }, startedAt: new Date().toISOString(), resumed: false }
}

export default function Questionnaire({ onDone }: { onDone: (r: Report) => void }) {
  const reduce = useReducedMotion()
  const boot = useRef(init())
  const [st, setSt] = useState<St>(boot.current.st)
  const [phase, setPhase] = useState<Phase>(() => {
    const b = boot.current
    const q = QUESTIONS.find((x) => x.id === b.st.currentId)
    return b.resumed || !q ? { kind: 'question' } : { kind: 'section', module: q.module }
  })
  const [dir, setDir] = useState(1)
  const enteredAt = useRef(Date.now())

  const vis = useMemo(() => visibleQuestions(QUESTIONS, st.answers, st.probeIds), [st.answers, st.probeIds])
  const idx = Math.max(0, vis.findIndex((q) => q.id === st.currentId))
  const q: Question | undefined = vis[idx]

  // Autosave every change.
  useEffect(() => {
    if (Object.keys(st.answers).length === 0) return
    const p: Progress = { ...st, startedAt: boot.current.startedAt, updatedAt: new Date().toISOString() }
    saveProgress(p)
  }, [st])

  useEffect(() => {
    enteredAt.current = Date.now()
    window.scrollTo(0, 0)
  }, [st.currentId, phase.kind])

  const commitTime = useCallback((): Record<string, number> => {
    if (!q || phase.kind !== 'question') return st.timings
    const spent = Date.now() - enteredAt.current
    enteredAt.current = Date.now()
    return { ...st.timings, [q.id]: (st.timings[q.id] ?? 0) + Math.min(spent, 120000) }
  }, [q, phase.kind, st.timings])

  const setAnswer = useCallback(
    (v: AnswerValue) => {
      if (!q) return
      setSt((s) => ({ ...s, answers: { ...s.answers, [q.id]: v } }))
    },
    [q],
  )

  const finish = useCallback(
    (answers: Answers, timings: Record<string, number>) => {
      setPhase({ kind: 'building' })
      window.setTimeout(() => {
        const report = buildReport(QUESTIONS, answers, timings)
        saveReport(report)
        addSnapshot(report)
        onDone(report)
      }, 900)
    },
    [onDone],
  )

  const goNext = useCallback(() => {
    if (!q) return
    const timings = commitTime()
    const next = vis[idx + 1]
    setDir(1)
    if (next) {
      setSt((s) => ({ ...s, timings, currentId: next.id }))
      if (next.module !== q.module && !next.probeFor) setPhase({ kind: 'section', module: next.module })
      else setPhase({ kind: 'question' })
      return
    }
    if (st.probeIds.length === 0) {
      const probes = selectProbes(QUESTIONS, st.answers)
      if (probes.length) {
        setSt((s) => ({ ...s, timings, probeIds: probes, currentId: probes[0] }))
        setPhase({ kind: 'probes' })
        return
      }
    }
    setSt((s) => ({ ...s, timings }))
    finish(st.answers, timings)
  }, [q, vis, idx, st.probeIds.length, st.answers, commitTime, finish])

  const goBack = useCallback(() => {
    if (phase.kind === 'building') return
    const timings = commitTime()
    if (idx <= 0) {
      if (phase.kind !== 'question') setPhase({ kind: 'question' })
      return
    }
    setDir(-1)
    setSt((s) => ({ ...s, timings, currentId: vis[idx - 1].id }))
    setPhase({ kind: 'question' })
  }, [phase.kind, idx, vis, commitTime])

  const canProceed = !!q && (isAnswered(st.answers[q.id]) || q.type === 'text' || !!q.optional)

  // Keyboard navigation.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement
      if (t && (t.tagName === 'TEXTAREA' || t.tagName === 'INPUT')) return
      if (e.metaKey || e.ctrlKey || e.altKey) return
      if (phase.kind === 'section' || phase.kind === 'probes') {
        if (e.key === 'Enter') {
          e.preventDefault()
          setPhase({ kind: 'question' })
        } else if (e.key === 'Backspace' || e.key === 'ArrowLeft') goBack()
        return
      }
      if (phase.kind !== 'question' || !q) return
      if (e.key === 'Enter' || e.key === 'ArrowRight') {
        if (e.key === 'Enter' && t && t.tagName === 'BUTTON') {
          const role = t.getAttribute('role')
          const selectedOption = (role === 'radio' || role === 'checkbox') && t.getAttribute('aria-checked') === 'true'
          if (!selectedOption) return // let the focused button do its normal job
        }
        if (canProceed) {
          e.preventDefault()
          goNext()
        }
      } else if (e.key === 'Backspace' || e.key === 'ArrowLeft') {
        e.preventDefault()
        goBack()
      } else if (/^[0-9]$/.test(e.key)) {
        const d = Number(e.key)
        if (q.type === 'likert') {
          if (d >= 1 && d <= (q.points ?? 5)) setAnswer(d)
        } else if (q.type === 'scale10') {
          setAnswer(d === 0 ? 10 : d)
        } else if ((q.type === 'choice' || q.type === 'forced') && q.options?.[d - 1]) {
          setAnswer(q.options[d - 1].id)
        } else if (q.type === 'multi' && q.options?.[d - 1]) {
          const id = q.options[d - 1].id
          const cur = Array.isArray(st.answers[q.id]) ? (st.answers[q.id] as string[]) : []
          if (cur.includes(id)) setAnswer(cur.filter((x) => x !== id))
          else if (cur.length < (q.maxSelect ?? 99)) setAnswer([...cur, id])
        }
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [phase, q, canProceed, goNext, goBack, setAnswer, st.answers])

  // Progress numbers.
  const answeredCount = vis.filter((x) => isAnswered(st.answers[x.id])).length
  const overall = vis.length ? Math.round((answeredCount / vis.length) * 100) : 0
  const secQs = q ? vis.filter((x) => (q.probeFor ? !!x.probeFor : x.module === q.module && !x.probeFor)) : []
  const secDone = secQs.filter((x) => isAnswered(st.answers[x.id])).length
  const secInfo = q && !q.probeFor ? SECTIONS.find((s) => s.module === q.module) : undefined
  const secTitle = q?.probeFor ? 'Follow-ups' : secInfo?.title ?? ''

  if (!q) return <div className="p-8">No questions available.</div>

  const slide = reduce ? {} : { initial: { opacity: 0, x: dir * 40 }, animate: { opacity: 1, x: 0 }, exit: { opacity: 0, x: dir * -40 }, transition: { duration: 0.28 } }

  return (
    <div className="min-h-screen flex flex-col">
      <header className="sticky top-0 z-20 bg-bg/85 backdrop-blur-md">
        <div className="h-1 bg-line" role="progressbar" aria-label="Overall progress" aria-valuemin={0} aria-valuemax={100} aria-valuenow={overall}>
          <div className="h-full bg-accent transition-all duration-500" style={{ width: `${overall}%` }} />
        </div>
        <div className="mx-auto max-w-2xl px-4 sm:px-6 py-3 flex items-center justify-between gap-3 text-sm">
          <button type="button" onClick={() => go('/')} className="text-muted hover:text-ink rounded px-1">
            Save and exit
          </button>
          {phase.kind !== 'building' && (
            <span className="text-faint tabular-nums" aria-live="polite">
              {overall}% done
            </span>
          )}
        </div>
      </header>

      <main className="flex-1 mx-auto w-full max-w-2xl px-4 sm:px-6 pb-40 pt-4 sm:pt-10">
        <AnimatePresence mode="wait" custom={dir}>
          {phase.kind === 'section' && (
            <motion.section key={`s-${phase.module}`} {...slide} className="pt-6 sm:pt-16">
              {(() => {
                const s = SECTIONS.find((x) => x.module === phase.module)
                return (
                  <>
                    <Eyebrow>Next up</Eyebrow>
                    <h1 className="mt-2 text-4xl sm:text-5xl font-semibold tracking-tight">{s?.title}</h1>
                    <p className="mt-2 text-xl text-muted">{s?.subtitle}</p>
                    <p className="mt-6 text-lg leading-relaxed text-muted">{s?.intro}</p>
                    <p className="mt-4 text-sm text-faint">About {s?.estMinutes} minutes</p>
                    <div className="mt-10 flex gap-3">
                      <Button size="lg" onClick={() => setPhase({ kind: 'question' })} data-nav>
                        Continue
                      </Button>
                      {idx > 0 && (
                        <Button variant="ghost" size="lg" onClick={goBack}>
                          Back
                        </Button>
                      )}
                    </div>
                  </>
                )
              })()}
            </motion.section>
          )}

          {phase.kind === 'probes' && (
            <motion.section key="probes" {...slide} className="pt-6 sm:pt-16">
              <Eyebrow>Nearly there</Eyebrow>
              <h1 className="mt-2 text-4xl sm:text-5xl font-semibold tracking-tight">A few follow-ups to sharpen your results</h1>
              <p className="mt-6 text-lg leading-relaxed text-muted">
                Some of your answers left a few things unclear. These {st.probeIds.length} extra questions are chosen just for you, and they should take a couple of minutes.
              </p>
              <div className="mt-10 flex gap-3">
                <Button size="lg" onClick={() => setPhase({ kind: 'question' })} data-nav>
                  Continue
                </Button>
                <Button variant="ghost" size="lg" onClick={goBack}>
                  Back
                </Button>
              </div>
            </motion.section>
          )}

          {phase.kind === 'building' && (
            <motion.section key="building" {...slide} className="pt-24 text-center" aria-live="polite">
              <div className="mx-auto h-10 w-10 rounded-full border-2 border-accent border-t-transparent animate-spin" aria-hidden />
              <h1 className="mt-8 text-3xl font-semibold tracking-tight">Putting your results together</h1>
              <p className="mt-2 text-muted">This happens on your device. Nothing is sent anywhere.</p>
            </motion.section>
          )}

          {phase.kind === 'question' && (
            <motion.section key={q.id} {...slide} aria-labelledby="q-text">
              <div className="mb-6 sm:mb-8">
                <div className="flex items-center justify-between text-sm text-faint">
                  <span>{secTitle}</span>
                  <span className="tabular-nums">
                    {Math.min(secDone + (isAnswered(st.answers[q.id]) ? 0 : 1), secQs.length)} of {secQs.length}
                  </span>
                </div>
                <div className="mt-2 h-1 rounded-full bg-line overflow-hidden" role="progressbar" aria-label="Section progress" aria-valuemin={0} aria-valuemax={100} aria-valuenow={secQs.length ? Math.round((secDone / secQs.length) * 100) : 0}>
                  <div className="h-full bg-faint transition-all duration-500" style={{ width: `${secQs.length ? (secDone / secQs.length) * 100 : 0}%` }} />
                </div>
              </div>
              <h1 id="q-text" className="text-[26px] sm:text-4xl font-semibold tracking-tight leading-tight">
                {q.text}
              </h1>
              {q.help && <p className="mt-3 text-lg text-muted">{q.help}</p>}
              <div className="mt-7 sm:mt-9" data-qinput>
                <QuestionInput key={q.id} q={q} value={st.answers[q.id]} onChange={setAnswer} />
              </div>
            </motion.section>
          )}
        </AnimatePresence>
      </main>

      {phase.kind === 'question' && (
        <div className="fixed bottom-0 inset-x-0 z-20 border-t border-line bg-bg/90 backdrop-blur-md pb-[env(safe-area-inset-bottom)]">
          <div className="mx-auto max-w-2xl px-4 sm:px-6 py-3 flex items-center justify-between gap-3">
            <Button variant="secondary" onClick={goBack} disabled={idx === 0} data-nav>
              Back
            </Button>
            <div className="flex items-center gap-2">
              {!canProceed && (
                <Button variant="ghost" onClick={goNext} data-nav>
                  Skip
                </Button>
              )}
              <Button onClick={goNext} disabled={!canProceed} data-nav>
                {idx === vis.length - 1 ? 'Finish' : 'Next'}
              </Button>
            </div>
          </div>
          <p className="hidden sm:block pb-2 text-center text-xs text-faint">Number keys choose, Enter continues, Backspace goes back</p>
        </div>
      )}
    </div>
  )
}
