import type { Question } from '../../engine/types'
import { choice, forced, likert, multi, ranking, scale10, textQ } from '../helpers'

const M = 'B' as const

export const B: Question[] = [
  ranking('B_rank', M, 'Rank these from most to least important for the life you want. There is no right order.', [
    ['wealth', 'Wealth and financial success', { financialDrive: 1, valWealth: 0.6, extrinsicMotivation: 0.4, idealFinances: 0.5, incomeExpectation: 0.4 }],
    ['freedom', 'Freedom and independence', { freedomNeed: 1, valFreedom: 0.6, autonomy: 0.4, valIndependence: 0.4, workAutonomyNeed: 0.5 }],
    ['relationships', 'Close relationships', { relatedness: 0.8, familyOrientation: 0.6, companionshipNeed: 0.6, idealRelationships: 0.5 }],
    ['achievement', 'Achievement and being excellent at something', { achievement: 1, valAchievement: 0.6, performanceOrientation: 0.3, idealCareer: 0.5, perfectionism: 0.2 }],
    ['creativity', 'Creativity and self-expression', { creativity: 0.8, valCreativity: 0.6, intrinsicMotivation: 0.3, idealGrowth: 0.4 }],
    ['stability', 'Stability and security', { stabilityNeed: 1, valSecurity: 0.6, securityNeed: 0.5, riskTolerance: 0.15 }],
    ['adventure', 'Adventure and new experiences', { adventure: 1, riskTolerance: 0.6, curiosity: 0.5, idealLeisure: 0.4 }],
    ['contribution', 'Contributing to others or the world', { contribution: 1, valCompassion: 0.5, meaning: 0.4, valCommunity: 0.4, helpingDrive: 0.4 }],
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
    ['public', 'Doing good work that many people celebrate', { societalExpectation: 0.6, recognitionNeed: 0.9, valRecognition: 0.8, extrinsicMotivation: 0.8, performanceOrientation: 0.7 }],
    { pairWith: 'B_nobody' },
  ),
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
]
