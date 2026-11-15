import type { Question } from '../../engine/types'
import { choice, forced, likert, multi, textQ } from '../helpers'

const M = 'A' as const

// Mini-IPIP (Donnellan, Oswald, Baird & Lucas, 2006). Public-domain IPIP wording.
// Negative weights are reverse keyed. Emotional stability is the reverse of neuroticism.
const ip = (n: number, text: string, dims: Partial<Record<'extraversion' | 'agreeableness' | 'conscientiousness' | 'emotionalStability' | 'openness', number>>, extra: Partial<Question> = {}): Question =>
  likert(`A_ip${n}`, M, text, dims, { instrument: 'mini-ipip', minLabel: 'Very inaccurate', maxLabel: 'Very accurate', help: n === 1 ? 'How accurately does each statement describe you as you are now, not as you wish to be?' : undefined, ...extra })

export const A: Question[] = [
  ip(1, 'Am the life of the party.', { extraversion: 1 }),
  ip(2, 'Sympathize with others’ feelings.', { agreeableness: 1 }),
  ip(3, 'Get chores done right away.', { conscientiousness: 1 }),
  ip(4, 'Have frequent mood swings.', { emotionalStability: -1 }),
  ip(5, 'Have a vivid imagination.', { openness: 1 }),
  ip(6, 'Don’t talk a lot.', { extraversion: -1 }),
  ip(7, 'Am not interested in other people’s problems.', { agreeableness: -1 }),
  ip(8, 'Often forget to put things back in their proper place.', { conscientiousness: -1 }),
  ip(9, 'Am relaxed most of the time.', { emotionalStability: 1 }),
  ip(10, 'Am not interested in abstract ideas.', { openness: -1 }),
  ip(11, 'Talk to a lot of different people at parties.', { extraversion: 1 }),
  ip(12, 'Feel others’ emotions.', { agreeableness: 1 }),
  ip(13, 'Like order.', { conscientiousness: 1 }),
  ip(14, 'Get upset easily.', { emotionalStability: -1 }),
  ip(15, 'Have difficulty understanding abstract ideas.', { openness: -1 }),
  ip(16, 'Keep in the background.', { extraversion: -1 }),
  ip(17, 'Am not really interested in others.', { agreeableness: -1 }),
  ip(18, 'Make a mess of things.', { conscientiousness: -1 }),
  ip(19, 'Seldom feel blue.', { emotionalStability: 1 }),
  ip(20, 'Do not have a good imagination.', { openness: -1 }),

  likert('A_imp1', M, 'Most people act on a whim now and then. How true is it that you often buy things or make plans first and think about them afterwards?', { impulsivity: 1, impulseControl: -0.8, riskTolerance: 0.4 }),
  likert('A_assert1', M, 'I say so when I disagree, even if it might cause some friction.', { assertiveness: 1, conflictApproach: 0.5, selfAdvocacy: 0.4 }),
  forced(
    'A_forced1',
    M,
    'Which sounds more like you on a day off?',
    ['planner', 'I like a rough plan for the day and I stick to it', { structurePreference: 0.85, adaptability: 0.4, planning: 0.6 }],
    ['drifter', 'I like to see where the day takes me', { structurePreference: 0.2, adaptability: 0.75, riskTolerance: 0.6 }],
  ),
  forced(
    'A_forced2',
    M,
    'After a long, busy social event, which is closer to the truth?',
    ['recharged', 'I feel energised and want more', { extraversion: 0.88 }],
    ['drained', 'I feel drained and need time alone', { extraversion: 0.18 }],
    { pairWith: 'A_ip11' },
  ),
  choice(
    'A_sc1',
    M,
    'A friend invites you on a spontaneous weekend trip that leaves in two hours. You have nothing urgent on. You...',
    [
      ['go', 'Pack a bag and go', { riskTolerance: 0.8, adaptability: 0.85, impulsivity: 0.7, structurePreference: 0.2, adventure: 0.8 }],
      ['ask', 'Ask a few questions, then probably go', { riskTolerance: 0.55, adaptability: 0.6, impulsivity: 0.45, structurePreference: 0.5 }],
      ['maybe', 'Say you would love to, but only with more notice', { riskTolerance: 0.3, adaptability: 0.35, impulsivity: 0.2, structurePreference: 0.75 }],
      ['no', 'Say no, spontaneous plans stress you out', { riskTolerance: 0.15, adaptability: 0.15, impulsivity: 0.1, structurePreference: 0.9, adventure: 0.2 }],
    ],
  ),
  multi('A_words', M, 'Which of these words fit you well? Pick up to five.', 5, [
    ['curious', 'Curious', { curiosity: 0.85, openness: 0.75 }],
    ['reliable', 'Reliable', { conscientiousness: 0.85, followThrough: 0.8 }],
    ['adventurous', 'Adventurous', { adventure: 0.85, riskTolerance: 0.8 }],
    ['calm', 'Calm', { emotionalStability: 0.85, stress: 0.2 }],
    ['creative', 'Creative', { creativity: 0.9, artistic: 0.7 }],
    ['caring', 'Caring', { empathy: 0.85, agreeableness: 0.8 }],
    ['driven', 'Driven', { achievement: 0.85, growthDrive: 0.75 }],
    ['independent', 'Independent', { freedomNeed: 0.8, relationalIndependence: 0.8 }],
    ['sociable', 'Sociable', { extraversion: 0.85, socialConfidence: 0.7 }],
    ['analytical', 'Analytical', { investigative: 0.8, intellectualDrive: 0.8 }],
    ['sensitive', 'Sensitive', { empathy: 0.75, emotionalStability: 0.3 }],
    ['easygoing', 'Easygoing', { adaptability: 0.85, structurePreference: 0.3 }],
  ]),
]
