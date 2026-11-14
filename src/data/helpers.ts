// Small constructors so the question files stay readable. Pure data helpers.
import type { Condition, DimId, ModuleId, Option, Question } from '../engine/types'

export type Fx = Partial<Record<DimId, number>>
/** [id, label, effects?] */
export type Opt = [string, string, Fx?]

const mk = (o: Opt[]): Option[] => o.map(([id, label, effects]) => (effects ? { id, label, effects } : { id, label }))

type Extra = Partial<Question>

export const likert = (id: string, module: ModuleId, text: string, dims: Fx, extra: Extra = {}): Question => ({
  id,
  module,
  type: 'likert',
  text,
  dims,
  points: 5,
  minLabel: 'Strongly disagree',
  maxLabel: 'Strongly agree',
  instrument: 'custom',
  ...extra,
})

export const scale10 = (id: string, module: ModuleId, text: string, dims: Fx, minLabel: string, maxLabel: string, extra: Extra = {}): Question => ({
  id,
  module,
  type: 'scale10',
  text,
  dims,
  minLabel,
  maxLabel,
  instrument: 'custom',
  ...extra,
})

export const choice = (id: string, module: ModuleId, text: string, options: Opt[], extra: Extra = {}): Question => ({
  id,
  module,
  type: 'choice',
  text,
  options: mk(options),
  instrument: 'custom',
  ...extra,
})

export const forced = (id: string, module: ModuleId, text: string, a: Opt, b: Opt, extra: Extra = {}): Question => ({
  id,
  module,
  type: 'forced',
  text,
  options: mk([a, b]),
  instrument: 'custom',
  ...extra,
})

export const multi = (id: string, module: ModuleId, text: string, maxSelect: number, options: Opt[], extra: Extra = {}): Question => ({
  id,
  module,
  type: 'multi',
  text,
  maxSelect,
  options: mk(options),
  instrument: 'custom',
  ...extra,
})

export const ranking = (id: string, module: ModuleId, text: string, options: Opt[], extra: Extra = {}): Question => ({
  id,
  module,
  type: 'ranking',
  text,
  options: mk(options),
  instrument: 'custom',
  ...extra,
})

export const textQ = (id: string, module: ModuleId, text: string, placeholder: string, extra: Extra = {}): Question => ({
  id,
  module,
  type: 'text',
  text,
  placeholder,
  optional: true,
  instrument: 'custom',
  ...extra,
})

export const includes = (q: string, value: string): Condition => ({ q, op: 'includes', value })
export const anyOf = (q: string, values: string[]): Condition => ({ any: values.map((v) => includes(q, v)) })
