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
  p(likert('P_meaning', 'C', 'I know why I get up in the morning.', { meaning: 1 }), ['meaning']),
  p(likert('P_rel', 'E', 'The people in my life make me feel valued.', { relationshipQuality: 1 }), ['relationshipQuality', 'relatedness']),
  p(likert('P_stress', 'C', 'My day-to-day life feels manageable.', { stress: -1 }), ['stress']),
  p(likert('P_lonely', 'E', 'I often feel left out.', { loneliness: 1 }), ['loneliness']),
  p(likert('P_se', 'D', 'I treat myself with the kindness I would show a good friend.', { selfEsteem: 1 }), ['selfEsteem']),
  p(likert('P_eff', 'D', 'I believe I can learn almost any skill if I put in the effort.', { selfEfficacy: 1, masteryOrientation: 0.4 }), ['selfEfficacy']),
  p(likert('P_intr', 'B', 'I do my best work on things I find interesting, whatever the reward.', { intrinsicMotivation: 1 }), ['intrinsicMotivation']),
  p(likert('P_extr', 'B', 'Rewards, such as pay or praise, are a big part of what keeps me going.', { extrinsicMotivation: 1 }), ['extrinsicMotivation']),
  p(likert('P_ach', 'B', 'Being the best at what I do matters a lot to me.', { achievement: 1 }), ['achievement']),
  p(likert('P_fin', 'B', 'Having plenty of money would change my life more than almost anything else.', { financialDrive: 1 }), ['financialDrive']),
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
  p(
    choice(
      'P_close',
      'E',
      'You say closeness matters to you. When someone gets close and wants to know the real you, you...',
      [
        ['open', 'Open up gradually and enjoy it', { intimacyComfort: 0.9, attachmentAvoidance: 0.15, companionshipNeed: 0.75 }],
        ['wary', 'Want it, but hold back until I feel safe', { intimacyComfort: 0.45, attachmentAnxiety: 0.6, trust: 0.4 }],
        ['retreat', 'Tend to keep things light and keep some distance', { intimacyComfort: 0.2, attachmentAvoidance: 0.8 }],
      ],
    ),
    ['intimacyComfort', 'companionshipNeed'],
  ),
  p(likert('P_follow', 'I', 'When I am excited about a goal, I keep working on it long after the excitement fades.', { followThrough: 1, achievement: 0.3 }), ['followThrough', 'achievement']),
  p(likert('P_proc', 'I', 'I put off important things until the pressure builds.', { procrastination: 1 }), ['procrastination']),
  p(likert('P_goal', 'I', 'I know exactly what I am working toward this year.', { goalClarity: 1 }), ['goalClarity']),
  p(likert('P_cclar', 'G', 'I could describe my ideal job in some detail.', { careerClarity: 1 }), ['careerClarity']),
  p(likert('P_cfit', 'G', 'I feel I am in the right line of work for me.', { careerFit: 1, careerSatisfaction: 0.5 }), ['careerFit', 'careerSatisfaction']),
  p(likert('P_futclar', 'J', 'I can name the next two or three steps toward the life I want.', { futureClarity: 1, planning: 0.4 }), ['futureClarity']),
  p(likert('P_risk', 'A', 'I am willing to risk losing something to gain something better.', { riskTolerance: 1 }), ['riskTolerance']),
  p(likert('P_mastery', 'F', 'I enjoy the slow work of getting really good at something.', { motMastery: 1, motProgress: 0.5 }), ['motMastery', 'motProgress']),
]
