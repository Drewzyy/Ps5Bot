import { Reorder, useReducedMotion } from 'motion/react'
import { useState } from 'react'
import type { AnswerValue, Question } from '../engine/types'
import { cx } from './ui'

const LIKERT5 = ['Strongly disagree', 'Disagree', 'Neutral', 'Agree', 'Strongly agree']
const LIKERT7 = ['Strongly disagree', 'Disagree', 'Slightly disagree', 'Neutral', 'Slightly agree', 'Agree', 'Strongly agree']

interface Props {
  q: Question
  value: AnswerValue | undefined
  onChange: (v: AnswerValue) => void
}

const choiceBase =
  'w-full text-left rounded-2xl border px-4 py-3.5 sm:px-5 sm:py-4 transition-colors flex items-start gap-3 text-[17px] leading-snug'
const on = 'border-accent bg-accent/10 text-ink ring-1 ring-accent'
const off = 'border-line bg-surface hover:border-faint hover:bg-raised'

function Key({ n, active }: { n: number; active?: boolean }) {
  return (
    <span
      aria-hidden
      className={cx(
        'mt-0.5 hidden sm:inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md border text-xs font-medium',
        active ? 'border-accent text-accent' : 'border-line text-faint',
      )}
    >
      {n}
    </span>
  )
}

export function QuestionInput({ q, value, onChange }: Props) {
  switch (q.type) {
    case 'likert': {
      const pts = q.points ?? 5
      const labels = pts === 7 ? LIKERT7 : LIKERT5
      return (
        <div role="radiogroup" aria-label={q.text}>
          <div className="grid gap-2" style={{ gridTemplateColumns: `repeat(${pts}, minmax(0, 1fr))` }}>
            {labels.map((l, i) => {
              const n = i + 1
              const sel = value === n
              return (
                <button
                  key={n}
                  type="button"
                  role="radio"
                  aria-checked={sel}
                  aria-label={`${n}, ${l}`}
                  onClick={() => onChange(n)}
                  className={cx('h-14 sm:h-16 rounded-2xl border text-lg font-medium transition-colors', sel ? 'border-accent bg-accent text-white' : off)}
                >
                  {n}
                </button>
              )
            })}
          </div>
          <div className="mt-3 flex justify-between text-sm text-faint">
            <span>{q.minLabel ?? labels[0]}</span>
            <span>{q.maxLabel ?? labels[labels.length - 1]}</span>
          </div>
          <p aria-live="polite" className="mt-2 h-5 text-center text-sm text-muted">
            {typeof value === 'number' ? labels[value - 1] : ''}
          </p>
        </div>
      )
    }
    case 'scale10':
      return (
        <div role="radiogroup" aria-label={q.text}>
          <div className="grid grid-cols-5 sm:grid-cols-10 gap-2">
            {Array.from({ length: 10 }, (_, i) => i + 1).map((n) => (
              <button
                key={n}
                type="button"
                role="radio"
                aria-checked={value === n}
                onClick={() => onChange(n)}
                className={cx('h-14 rounded-2xl border text-lg font-medium transition-colors', value === n ? 'border-accent bg-accent text-white' : off)}
              >
                {n}
              </button>
            ))}
          </div>
          <div className="mt-3 flex justify-between text-sm text-faint">
            <span>{q.minLabel ?? 'Not at all'}</span>
            <span>{q.maxLabel ?? 'Completely'}</span>
          </div>
        </div>
      )
    case 'choice':
      return (
        <div role="radiogroup" aria-label={q.text} className="space-y-2.5">
          {q.options?.map((o, i) => {
            const sel = value === o.id
            return (
              <button key={o.id} type="button" role="radio" aria-checked={sel} onClick={() => onChange(o.id)} className={cx(choiceBase, sel ? on : off)}>
                <Key n={i + 1} active={sel} />
                <span>{o.label}</span>
              </button>
            )
          })}
        </div>
      )
    case 'forced':
      return (
        <div role="radiogroup" aria-label={q.text} className="grid gap-3 md:grid-cols-2">
          {q.options?.slice(0, 2).map((o, i) => {
            const sel = value === o.id
            return (
              <button
                key={o.id}
                type="button"
                role="radio"
                aria-checked={sel}
                onClick={() => onChange(o.id)}
                className={cx('rounded-card border p-6 sm:p-8 text-left text-xl sm:text-[22px] font-medium leading-snug min-h-[120px] md:min-h-[200px] transition-colors', sel ? on : off)}
              >
                <span className="block text-sm font-medium text-faint mb-3">{i === 0 ? 'Option 1' : 'Option 2'}</span>
                {o.label}
              </button>
            )
          })}
        </div>
      )
    case 'multi': {
      const max = q.maxSelect ?? q.options?.length ?? 99
      const cur = Array.isArray(value) ? value : []
      const toggle = (id: string) => {
        if (cur.includes(id)) onChange(cur.filter((x) => x !== id))
        else if (cur.length < max) onChange([...cur, id])
      }
      return (
        <div>
          <p className="mb-3 text-sm text-faint" aria-live="polite">
            Pick up to {max}. {cur.length} chosen.
          </p>
          <div role="group" aria-label={q.text} className="space-y-2.5">
            {q.options?.map((o, i) => {
              const sel = cur.includes(o.id)
              const blocked = !sel && cur.length >= max
              return (
                <button
                  key={o.id}
                  type="button"
                  role="checkbox"
                  aria-checked={sel}
                  disabled={blocked}
                  onClick={() => toggle(o.id)}
                  className={cx(choiceBase, sel ? on : off, blocked && 'opacity-40')}
                >
                  <Key n={i + 1} active={sel} />
                  <span className="flex-1">{o.label}</span>
                  <span aria-hidden className={cx('mt-0.5 h-5 w-5 shrink-0 rounded-md border flex items-center justify-center text-xs', sel ? 'bg-accent border-accent text-white' : 'border-faint')}>
                    {sel ? '✓' : ''}
                  </span>
                </button>
              )
            })}
          </div>
        </div>
      )
    }
    case 'ranking':
      return <Ranking q={q} value={value} onChange={onChange} />
    case 'text':
      return (
        <div>
          <label htmlFor={`t-${q.id}`} className="sr-only">
            {q.text}
          </label>
          <textarea
            id={`t-${q.id}`}
            value={typeof value === 'string' ? value : ''}
            onChange={(e) => onChange(e.target.value)}
            rows={6}
            maxLength={2000}
            placeholder={q.placeholder ?? 'Write whatever comes to mind. You can leave this blank.'}
            className="w-full rounded-2xl border border-line bg-surface p-4 text-[17px] leading-relaxed placeholder:text-faint focus:border-accent"
          />
          <p className="mt-2 text-sm text-faint">Optional. This text is stored only on this device and is not scored.</p>
        </div>
      )
  }
}

function Ranking({ q, value, onChange }: Props) {
  const reduce = useReducedMotion()
  const opts = q.options ?? []
  const saved = Array.isArray(value) && value.length === opts.length ? value : null
  const [order, setOrder] = useState<string[]>(saved ?? opts.map((o) => o.id))
  const [live, setLive] = useState('')
  const label = (id: string) => opts.find((o) => o.id === id)?.label ?? id
  const commit = (next: string[]) => {
    setOrder(next)
    onChange(next)
  }
  const move = (i: number, d: -1 | 1) => {
    const j = i + d
    if (j < 0 || j >= order.length) return
    const next = [...order]
    ;[next[i], next[j]] = [next[j], next[i]]
    commit(next)
    setLive(`${label(order[i])} moved to position ${j + 1} of ${order.length}`)
  }
  return (
    <div>
      <p className="mb-3 text-sm text-faint">
        Most important at the top. Drag to reorder, or use the arrow buttons.{' '}
        {!saved && <span className="text-muted">Move something, or press the button below to keep this order.</span>}
      </p>
      <Reorder.Group axis="y" values={order} onReorder={commit} className="space-y-2" as="ol" aria-label={q.text}>
        {order.map((id, i) => (
          <Reorder.Item
            key={id}
            value={id}
            as="li"
            dragListener={!reduce}
            className="flex items-center gap-3 rounded-2xl border border-line bg-surface px-3 py-2.5 sm:px-4 cursor-grab active:cursor-grabbing"
          >
            <span className="w-6 text-center text-sm font-semibold text-accent tabular-nums">{i + 1}</span>
            <span className="flex-1 text-[17px] leading-snug">{label(id)}</span>
            <span className="flex gap-1">
              <button type="button" aria-label={`Move ${label(id)} up`} disabled={i === 0} onClick={() => move(i, -1)} className="h-9 w-9 rounded-full border border-line text-muted hover:bg-raised disabled:opacity-30">
                <span aria-hidden>&uarr;</span>
              </button>
              <button type="button" aria-label={`Move ${label(id)} down`} disabled={i === order.length - 1} onClick={() => move(i, 1)} className="h-9 w-9 rounded-full border border-line text-muted hover:bg-raised disabled:opacity-30">
                <span aria-hidden>&darr;</span>
              </button>
            </span>
          </Reorder.Item>
        ))}
      </Reorder.Group>
      <div className="sr-only" aria-live="polite">
        {live}
      </div>
      {!saved && (
        <button type="button" onClick={() => commit(order)} className="mt-4 rounded-full border border-line px-5 py-2 text-[15px] hover:bg-raised">
          Keep this order
        </button>
      )}
    </div>
  )
}
