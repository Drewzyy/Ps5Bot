import type { Question } from '../engine/types'
import { A } from './questions/A'
import { B } from './questions/B'
import { C } from './questions/C'
import { D } from './questions/D'
import { E } from './questions/E'
import { F } from './questions/F'
import { G } from './questions/G'
import { H } from './questions/H'
import { I } from './questions/I'
import { J } from './questions/J'
import { P } from './questions/P'

/** Base questions in flow order (A to J), followed by adaptive probes (those with `probeFor`). */
export const QUESTIONS: Question[] = [...A, ...B, ...C, ...D, ...E, ...F, ...G, ...H, ...I, ...J, ...P]

export const BASE_QUESTIONS: Question[] = QUESTIONS.filter((q) => !q.probeFor)
export const PROBE_QUESTIONS: Question[] = QUESTIONS.filter((q) => q.probeFor)
