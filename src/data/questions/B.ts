import type { Question } from '../../engine/types'
import { choice, forced, likert, multi, ranking, scale10, textQ } from '../helpers'

const M = 'B' as const

export const B: Question[] = [
  ranking('B_rank', M, 'Rank these from most to least important for the life you want. There is no right order.', [
    ['wealth', 'Wealth and financial success', { financialDrive: 1, valWealth: 0.6, extrinsicMotivation: 0.4, idealFinances: 0.5 }],
    ['freedom', 'Freedom and independence', { freedomNeed: 1, valFreedom: 0.6, autonomy: 0.4 }],
    ['relationships', 'Close relationships', { relatedness: 0.8, familyOrientation: 0.6, companionshipNeed: 0.6, idealRelationships: 0.5 }],
    ['achievement', 'Achievement and being excellent at something', { achievement: 1, valAchievement: 0.6, performanceOrientation: 0.3, idealCareer: 0.5 }],
    ['creativity', 'Creativity and self-expression', { creativity: 0.8, valCreativity: 0.6, intrinsicMotivation: 0.3, idealGrowth: 0.4 }],
    ['stability', 'Stability and security', { stabilityNeed: 1, valSecurity: 0.6, securityNeed: 0.5 }],
    ['adventure', 'Adventure and new experiences', { adventure: 1, riskTolerance: 0.4, curiosity: 0.4, idealLeisure: 0.4 }],
    ['contribution', 'Contributing to others or the world', { contribution: 1, valCompassion: 0.5, meaning: 0.4 }],
  ]),
  likert('B_nobody', M, 'I would still pursue my biggest goal even if nobody ever knew I had achieved it.', { intrinsicMotivation: 1, extrinsicMotivation: -0.7, recognitionNeed: -0.8, societalExpectation: -0.4 }),
  forced(
    'B_career',
    M,
    'If you had to choose, which would you pick?',
    ['bigdisliked', 'Be very successful in a career I dislike', { extrinsicMotivation: 0.85, financialDrive: 0.75, prestigeNeed: 0.7, intrinsicMotivation: 0.2, societalExpectation: 0.6 }],
    ['modestloved', 'Be moderately successful in a career I love', { intrinsicMotivation: 0.9, extrinsicMotivation: 0.2, financialDrive: 0.3, careerFit: 0.8, societalExpectation: 0.2 }],
  ),
  forced(
    'B_recog',
    M,
    'Which outcome would feel better?',
    ['quiet', 'Doing excellent work that only a few people ever see', { recognitionNeed: 0.15, intrinsicMotivation: 0.85, masteryOrientation: 0.7 }],
    ['public', 'Doing good work that many people celebrate', { recognitionNeed: 0.9, extrinsicMotivation: 0.8, performanceOrientation: 0.7 }],
    { pairWith: 'B_nobody' },
  ),
  choice(
    'B_voice',
    M,
    'Think about your biggest ambition. Whose voice is loudest behind it?',
    [
      ['mine', 'Mine. I would want it whatever anyone thought', { intrinsicMotivation: 0.9, societalExpectation: 0.1 }],
      ['family', 'My family’s hopes or expectations', { societalExpectation: 0.8, familyOrientation: 0.7, intrinsicMotivation: 0.4 }],
      ['partner', 'People close to me, like a partner or friends', { societalExpectation: 0.5, relatedness: 0.7 }],
      ['society', 'What a “successful life” is supposed to look like', { societalExpectation: 0.95, extrinsicMotivation: 0.8, intrinsicMotivation: 0.25 }],
      ['unsure', 'I honestly cannot tell', { societalExpectation: 0.55, goalClarity: 0.25 }],
    ],
  ),
  likert('B_should', M, 'Many people chase goals they think they should want. How true is that for some of your goals?', { societalExpectation: 1, intrinsicMotivation: -0.6 }),
  scale10('B_money', M, 'How much does a high income matter to your idea of a good life?', { financialDrive: 1, valWealth: 0.8 }, 'Hardly at all', 'It matters enormously'),
  choice('B_raise', M, 'You are offered one of these in your next review. Which do you pick?', [
    ['pay', '20 percent more pay', { financialDrive: 0.85, extrinsicMotivation: 0.7, balancePriority: 0.25 }],
    ['time', '20 percent more time off', { freedomNeed: 0.85, balancePriority: 0.85, financialDrive: 0.3, workLifeBalance: 0.7 }],
    ['title', 'A better title and more responsibility', { prestigeNeed: 0.85, leadershipDrive: 0.7, recognitionNeed: 0.7, achievement: 0.7 }],
    ['learn', 'Paid time to learn something new', { growthDrive: 0.9, masteryOrientation: 0.8, intrinsicMotivation: 0.7 }],
  ]),
  multi('B_goals', M, 'Which of these are real goals for you right now? Pick up to four.', 4, [
    ['advance', 'Move up or get recognised in my career', { achievement: 0.8, recognitionNeed: 0.7, prestigeNeed: 0.6, idealCareer: 0.85 }],
    ['money', 'Build financial security', { financialDrive: 0.75, stabilityNeed: 0.7, securityNeed: 0.6, idealFinances: 0.85 }],
    ['business', 'Start something of my own', { freedomNeed: 0.8, riskTolerance: 0.75, leadershipDrive: 0.6, influence: 0.5 }],
    ['health', 'Get healthier and fitter', { curHealth: 0.4, idealHealth: 0.85, growthDrive: 0.6, bodyImage: 0.45 }],
    ['love', 'Build or deepen close relationships', { relatedness: 0.8, companionshipNeed: 0.7, familyOrientation: 0.65, idealRelationships: 0.85 }],
    ['travel', 'Travel and see the world', { adventure: 0.9, curiosity: 0.7, freedomNeed: 0.6, idealLeisure: 0.8 }],
    ['make', 'Make something creative', { creativity: 0.85, intrinsicMotivation: 0.7, artistic: 0.6 }],
    ['learn', 'Learn a big new skill or subject', { growthDrive: 0.85, intellectualDrive: 0.75, masteryOrientation: 0.7, idealGrowth: 0.85 }],
    ['give', 'Help others or give back', { contribution: 0.85, helpingDrive: 0.7, valCompassion: 0.6 }],
    ['calm', 'Feel calmer and less pressured', { stabilityNeed: 0.6, balancePriority: 0.7, stress: 0.7 }],
  ]),
  textQ('B_t1', M, 'If money were guaranteed for the rest of your life, what would you pursue?', 'Optional. Be as specific or as vague as you like.'),
]
