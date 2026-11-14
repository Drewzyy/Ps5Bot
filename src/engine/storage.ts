// Local-only persistence. Nothing here talks to a network.

import type { LifePriorities, Progress, Report, Snapshot } from './types'

const K = {
  progress: 'thi.progress.v1',
  report: 'thi.report.v1',
  history: 'thi.history.v1',
  theme: 'thi.theme.v1',
  priorities: 'thi.priorities.v1',
}

function read<T>(key: string): T | null {
  try {
    const raw = localStorage.getItem(key)
    return raw ? (JSON.parse(raw) as T) : null
  } catch {
    return null
  }
}
function write(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    /* storage unavailable: the app still works for the current session */
  }
}

export const loadProgress = () => read<Progress>(K.progress)
export const saveProgress = (p: Progress) => write(K.progress, { ...p, updatedAt: new Date().toISOString() })
export const clearProgress = () => localStorage.removeItem(K.progress)

export const loadReport = () => read<Report>(K.report)
export const saveReport = (r: Report) => write(K.report, r)

export const loadHistory = () => read<Snapshot[]>(K.history) ?? []
export function addSnapshot(r: Report): Snapshot {
  const snap: Snapshot = {
    id: `${Date.now()}`,
    takenAt: r.generatedAt,
    scores: Object.fromEntries(Object.entries(r.scores).map(([k, v]) => [k, v.score])),
    overallConfidence: r.quality.overallConfidence,
    careerAlignment: r.career.alignmentScore,
    happiness: r.happiness.overall,
    topValues: r.values.ranked.slice(0, 5).map((v) => v.label),
  }
  write(K.history, [...loadHistory(), snap])
  return snap
}
export const deleteSnapshot = (id: string) => write(K.history, loadHistory().filter((s) => s.id !== id))

export type Theme = 'light' | 'dark' | 'system'
export const loadTheme = (): Theme => read<Theme>(K.theme) ?? 'system'
export function applyTheme(t: Theme): void {
  write(K.theme, t)
  const dark = t === 'dark' || (t === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
  document.documentElement.dataset.theme = dark ? 'dark' : 'light'
}

export const loadPriorities = () => read<LifePriorities>(K.priorities)
export const savePriorities = (p: LifePriorities) => write(K.priorities, p)

/** Everything stored on this device, for the "export my data" control. */
export function exportAll(): string {
  return JSON.stringify(
    { exportedAt: new Date().toISOString(), progress: loadProgress(), report: loadReport(), history: loadHistory() },
    null,
    2,
  )
}
export function deleteAll(): void {
  Object.values(K).forEach((k) => k !== K.theme && localStorage.removeItem(k))
}
