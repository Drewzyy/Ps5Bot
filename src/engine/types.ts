// Shared contract for The Human Index. Question data, the scoring engine, the
// interpretation engine and the UI all depend on these types. Do not change
// them without updating every consumer.

import type { DimId } from './dimensions'
export type { DimId }

export type ModuleId = 'A' | 'B' | 'C' | 'D' | 'E' | 'F' | 'G' | 'H' | 'I' | 'J'

export type QuestionType =
  | 'likert' // agreement scale, `points` 5 or 7. Answer: number 1..points
  | 'scale10' // 1..10 rating. Answer: number 1..10
  | 'choice' // single choice (also scenario / situational judgment). Answer: option id
  | 'forced' // exactly two options. Answer: option id
  | 'multi' // pick several (max `maxSelect`). Answer: option id[]
  | 'ranking' // order all options, most important first. Answer: option id[] in rank order
  | 'text' // open written response. Not scored. Answer: string

export type AnswerValue = number | string | string[]
export type Answers = Record<string, AnswerValue>

export interface Option {
  id: string
  label: string
  /**
   * choice / forced / multi: the level (0..1) this option indicates for each dimension.
   *   0 = very low on the dimension, 0.5 = neutral, 1 = very high.
   * ranking: the weight (0..1) with which this item's rank position feeds each dimension
   *   (rank 1 => level 1, last rank => level 0).
   */
  effects?: Partial<Record<DimId, number>>
}

/** Declarative visibility rule so that adaptive logic is data, not code. */
export type Condition =
  | { q: string; op: 'eq' | 'neq' | 'gte' | 'lte' | 'includes'; value: number | string }
  | { dim: DimId; op: 'gte' | 'lte'; value: number } // provisional 0..100 score from answers so far
  | { all: Condition[] }
  | { any: Condition[] }

export interface Question {
  id: string // e.g. "A_bf_e1", unique
  module: ModuleId
  type: QuestionType
  text: string
  help?: string // short clarification shown under the question
  /** likert / scale10: dimension -> weight. Negative weight = reverse keyed for that dimension. */
  dims?: Partial<Record<DimId, number>>
  points?: 5 | 7 // likert only, default 5
  minLabel?: string
  maxLabel?: string
  options?: Option[]
  maxSelect?: number // multi only
  weight?: number // overall weight multiplier, default 1
  /** Shown only when the condition holds. Omit for always-on questions. */
  showIf?: Condition
  /**
   * Adaptive probe. Probes are not part of the base flow; the engine picks them
   * at the end to raise confidence on these dimensions or to examine a contradiction.
   */
  probeFor?: DimId[]
  /** Source instrument. Anything other than 'custom' must be documented in the knowledge base. */
  instrument?: 'mini-ipip' | 'swls' | 'custom'
  /** Id of another question measuring the same thing; used for response-consistency checks. */
  pairWith?: string
  optional?: boolean // text questions are optional by default
  placeholder?: string
}

export interface Section {
  module: ModuleId
  title: string
  subtitle: string
  intro: string // 1-3 sentences shown on the section intro screen
  estMinutes: number
}

// ---------- scoring output ----------

export interface Contribution {
  questionId: string
  level: number // 0..1 what this answer indicated for the dimension
  weight: number
}

export interface DimScore {
  score: number // 0..100
  confidence: number // 0..1, never reported above 0.97
  n: number // number of observations
  contributions: Contribution[]
}

export type Scores = Record<DimId, DimScore>

export interface Quality {
  answered: number
  shown: number
  overallConfidence: number // 0..1
  flags: { id: 'straightlining' | 'inconsistent-pairs' | 'speeding' | 'many-skipped'; detail: string }[]
}

// ---------- interpretation output ----------

/** Every interpretive claim carries the dimensions it was derived from so the UI can explain it. */
export interface Insight {
  title: string
  body: string
  evidence: DimId[]
}

export interface Contradiction {
  id: string
  title: string // e.g. "Wants financial freedom, avoids financial risk"
  a: { dim: DimId; label: string }
  b: { dim: DimId; label: string }
  explanation: string // framed as a hypothesis / reflection prompt
  question: string // a question for the user to sit with
  strength: number // 0..1
}

export interface HobbyMatch {
  id: string
  name: string
  matchScore: number // 0..100 "discovery score"
  why: string
  mightEnjoy: string
  mightDislike: string
  difficulty: 'Easy' | 'Moderate' | 'Demanding'
  cost: string // e.g. "$0-50 to start"
  firstStep: string
  motives: DimId[] // the hobby-motive dimensions that drove the match
  alreadyDoesSimilar?: boolean
}

export interface CareerMatch {
  id: string
  title: string
  matchScore: number // 0..100
  riasec: string // e.g. "IAS"
  why: string
  tradeoffs: string
}

export interface AlignmentComponent {
  label: string
  score: number // 0..100
  explanation: string
}

export interface PopulationComparison {
  id: string
  label: string
  userValue: string
  band: string // where the user falls
  source: string
  sample: string
  population: string
  instrument: string
  limitations: string
}

export interface DomainGap {
  domain: string // Career, Relationships, Health, Finances, Growth, Leisure ...
  current: number // 0..100
  ideal: number // 0..100
}

export interface Report {
  generatedAt: string // ISO
  scores: Scores
  quality: Quality

  identity: {
    headline: string // one line, specific to this profile
    overview: string[] // paragraphs
    coreTraits: Insight[]
    behavioral: string
    social: string
    decisionStyle: string
    strengths: Insight[]
    blindSpots: Insight[] // worded as possibilities
  }
  typeSketch: {
    code: string // four letters, MBTI-style, derived from Big Five
    axes: { axis: string; letter: string; strength: number }[] // strength 0..1 distance from midpoint
    description: string
    caveat: string // must state this is not a validated measure
  }
  ambitions: {
    drivers: { dim: DimId; label: string; score: number; why: string }[] // strongest first
    overview: string[]
    intrinsicVsExtrinsic: { intrinsic: number; extrinsic: number; reading: string }
    statedVsRevealed: Insight[] // gaps between what is said, what behaviour suggests, what is expected
    conflicts: Insight[]
    overlooked: Insight[]
  }
  happiness: {
    overall: number // 0..100 composite
    perma: { dim: DimId; label: string; score: number }[]
    lifts: Insight[] // what seems to contribute most
    drags: Insight[]
    stateVsTrait: string
    overview: string[]
  }
  confidence: {
    overview: string[]
    profile: { dim: DimId; label: string; score: number }[]
    hypotheses: Insight[]
  }
  relationships: {
    overview: string[]
    profile: { dim: DimId; label: string; score: number }[]
    strengths: Insight[]
    needs: Insight[]
    growth: Insight[]
    currentVsIdeal: string
  }
  career: {
    alignmentScore: number
    components: AlignmentComponent[]
    riasecCode: string
    riasec: { dim: DimId; label: string; score: number }[]
    environment: string
    conflicts: Insight[]
    alternatives: CareerMatch[]
    overview: string[]
  }
  values: {
    ranked: { dim: DimId; label: string; score: number }[]
    conflicts: Insight[]
    overview: string
  }
  habits: {
    overview: string[]
    profile: { dim: DimId; label: string; score: number }[]
    /** Which of the four explanations fits best: motivation, unclear goals, environment, competing priorities. */
    bottleneck: { kind: 'motivation' | 'clarity' | 'environment' | 'priorities' | 'none'; reading: string }
  }
  future: {
    overview: string[]
    gaps: DomainGap[]
    trajectory: Insight[]
  }
  hobbies: HobbyMatch[] // at least 10, best first
  motives: { dim: DimId; label: string; score: number }[] // hobby motive profile
  contradictions: Contradiction[]
  hiddenPotential: Insight[]
  roadmap: {
    short: Insight[] // 3
    medium: Insight[] // 3
    long: Insight[] // 3
    experiences: string[]
    habits: string[]
    reflections: string[]
    skills: string[]
  }
  summary: string[] // closing narrative paragraphs
  population: PopulationComparison[]
}

// ---------- persistence ----------

export interface Progress {
  answers: Answers
  timings: Record<string, number> // ms spent per question
  currentId: string | null
  probeIds: string[] // adaptive probes chosen so far
  startedAt: string
  updatedAt: string
}

export interface Snapshot {
  id: string
  takenAt: string
  scores: Record<string, number> // DimId -> 0..100 (scores only, no answers)
  overallConfidence: number
  careerAlignment: number
  happiness: number
  topValues: string[]
}

export interface LifePriorities {
  wealth: number
  freedom: number
  relationships: number
  achievement: number
  creativity: number
  stability: number
  adventure: number
  contribution: number
}
