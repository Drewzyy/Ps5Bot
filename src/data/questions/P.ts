// Adaptive probes. Not part of the base flow: the engine picks these at the end to raise
// confidence on weakly measured dimensions or to look at an apparent contradiction.
import type { Question } from '../../engine/types'
import { choice, likert, scale10 } from '../helpers'

const p = (q: Question, probeFor: Question['probeFor']): Question => ({ ...q, probeFor, optional: true })

export const P: Question[] = [
  p(likert('P_ext', 'A', 'I feel energised by being around lots of people.', { extraversion: 1 }), ['extraversion']),
  p(likert('P_agr', 'A', 'I go out of my way to be kind, even when it is inconvenient.', { agreeableness: 1 }), ['agreeableness']),
  p(likert('P_con', 'A', 'I follow through on plans even when I do not feel like it.', { conscientiousness: 1 }), ['conscientiousness']),
  p(likert('P_stab', 'A', 'I stay fairly steady when things go wrong.', { emotionalStability: 1 }), ['emotionalStability']),
  p(likert('P_ope', 'A', 'I enjoy ideas and experiences that are unusual or unfamiliar.', { openness: 1 }), ['openness']),
  p(scale10('P_life', 'C', 'Overall, taking everything together, how satisfied are you with your life as a whole?', { lifeSatisfaction: 1 }, 'Not at all', 'Completely'), ['lifeSatisfaction']),
  p(likert('P_se', 'D', 'I treat myself with the kindness I would show a good friend.', { selfEsteem: 1 }), ['selfEsteem']),
  p(likert('P_intr', 'B', 'I do my best work on things I find interesting, whatever the reward.', { intrinsicMotivation: 1 }), ['intrinsicMotivation']),
  p(likert('P_extr', 'B', 'Rewards, such as pay or praise, are a big part of what keeps me going.', { extrinsicMotivation: 1 }), ['extrinsicMotivation']),
  p(
    choice(
      'P_fin_risk',
      'B',
      'You want financial freedom. Which route sounds most like you?',
      [
        ['safe', 'Save steadily and avoid anything that could lose money', { riskTolerance: 0.15, stabilityNeed: 0.85, financialDrive: 0.6 }],
        ['mixed', 'Mostly steady, with a small share in riskier bets', { riskTolerance: 0.5, financialDrive: 0.65 }],
        ['bold', 'Take big swings on a business or investments', { riskTolerance: 0.9, financialDrive: 0.8 }],
      ],
    ),
    ['financialDrive', 'riskTolerance'],
  ),
]
