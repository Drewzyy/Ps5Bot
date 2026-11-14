import type { Question, AnswerValue } from '../engine/types'

export const pct = (n: number) => `${Math.round(n)}`
export const band = (score: number) => (score >= 67 ? 'high' : score >= 34 ? 'middle' : 'low')

export function confLabel(c: number): 'High' | 'Moderate' | 'Low' | 'Very low' {
  if (c >= 0.75) return 'High'
  if (c >= 0.55) return 'Moderate'
  if (c >= 0.3) return 'Low'
  return 'Very low'
}

export function describeAnswer(q: Question | undefined, v: AnswerValue | undefined): string {
  if (!q || v === undefined) return 'No answer recorded'
  if (q.type === 'likert') {
    const pts = q.points ?? 5
    return `${v} of ${pts}${q.minLabel && q.maxLabel ? ` (${q.minLabel} to ${q.maxLabel})` : ''}`
  }
  if (q.type === 'scale10') return `${v} of 10`
  if (q.type === 'text') return typeof v === 'string' ? v : String(v)
  const label = (id: string) => q.options?.find((o) => o.id === id)?.label ?? id
  if (Array.isArray(v)) {
    if (q.type === 'ranking') return v.map((id, i) => `${i + 1}. ${label(id)}`).join('  ')
    return v.map(label).join(', ')
  }
  return label(String(v))
}
