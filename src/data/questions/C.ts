import type { Question } from '../../engine/types'
import { choice, likert, multi, scale10, textQ } from '../helpers'

const M = 'C' as const

// Satisfaction With Life Scale (Diener, Emmons, Larsen & Griffin, 1985). Verbatim items, 7-point agreement.
const sw = (n: number, text: string): Question =>
  likert(`C_sw${n}`, M, text, { lifeSatisfaction: 1 }, {
    points: 7,
    instrument: 'swls',
    minLabel: 'Strongly disagree',
    maxLabel: 'Strongly agree',
    help: n === 1 ? 'Below are five statements. Indicate how much you agree with each, thinking about your life as a whole.' : undefined,
  })

export const C: Question[] = [
  sw(1, 'In most ways my life is close to my ideal.'),
  sw(2, 'The conditions of my life are excellent.'),
  sw(3, 'I am satisfied with my life.'),
  sw(4, 'So far I have gotten the important things I want in life.'),
  sw(5, 'If I could live my life over, I would change almost nothing.'),

  scale10('C_mood1', M, 'Thinking only about the past two weeks, how has your mood been overall?', { recentMood: 1 }, 'Low most days', 'Good most days', {
    help: 'This is about the last two weeks specifically, not your life in general.',
  }),
  likert('C_pe1', M, 'In an ordinary week I regularly feel joy, interest or contentment.', { positiveEmotion: 1, gratitude: 0.4, engagement: 0.4 }),
  likert('C_rel1', M, 'When I need support, there are people I can count on.', { relationshipQuality: 1, relatedness: 0.5, trust: 0.3, loneliness: -0.3 }),
  likert('C_mea1', M, 'My life has a sense of direction or purpose that matters to me.', { meaning: 1, optimism: 0.3, curHealth: 0.2 }),
  likert('C_acc1', M, 'I regularly feel I am making real progress on things that matter to me.', { accomplishment: 1, competence: 0.6, curGrowth: 0.4, engagement: 0.3 }),
  likert('C_stress1', M, 'Almost everyone feels overloaded at times. In the past month, how often have the demands on you felt like more than you could cope with?', { stress: 1, workLifeBalance: -0.5, curHealth: -0.3 }, { help: 'Agree means this has been true quite often.' }),
  likert('C_lonely1', M, 'Even when I am around people, I sometimes feel isolated.', { loneliness: 1, feelingUnderstood: -0.3 }, { pairWith: 'E_lonely2' }),
  likert('C_grat1', M, 'I often notice and appreciate small good things in my day.', { gratitude: 1, positiveEmotion: 0.4, optimism: 0.7, futureOptimism: 0.3 }),
  // Adaptive follow-ups
  choice(
    'C_low1',
    M,
    'Your answers suggest life has not felt great lately. Which of these weighs on you most right now?',
    [
      ['work', 'Work or study', { careerSatisfaction: 0.2, stress: 0.7 }],
      ['money', 'Money worries', { curFinances: 0.15, stress: 0.8 }],
      ['people', 'Relationships or feeling alone', { relationshipQuality: 0.2, loneliness: 0.8 }],
      ['health', 'My health or energy', { curHealth: 0.15, recentMood: 0.3 }],
      ['direction', 'Not knowing where my life is heading', { futureClarity: 0.15, meaning: 0.25, goalClarity: 0.25 }],
      ['self', 'How I feel about myself', { selfEsteem: 0.2 }],
      ['nothing', 'Nothing in particular, it is a general flatness', { positiveEmotion: 0.2, meaning: 0.3 }],
    ],
    { showIf: { dim: 'lifeSatisfaction', op: 'lte', value: 40 } },
  ),
  likert('C_low3', M, 'Lately it has been hard to enjoy things I normally like.', { positiveEmotion: -1, recentMood: -0.6, engagement: -0.4 }, {
    help: 'This is a reflection tool, not a diagnosis. If low mood is lasting or heavy, talking to someone you trust or a health professional can really help.',
    showIf: { dim: 'recentMood', op: 'lte', value: 35 },
  }),
]
