import type { Question } from '../../engine/types'
import { choice, likert, scale10, textQ } from '../helpers'

const M = 'E' as const

export const E: Question[] = [
  likert('E_anx1', M, 'I sometimes worry that people I care about will lose interest in me.', { attachmentAnxiety: 1, fearOfRejection: 0.6, validationNeed: 0.3 }),
  likert('E_emp1', M, 'I can usually tell how someone feels before they say it.', { empathy: 1 }),
  choice('E_conf1', M, 'A close friend says something that hurts you. What do you usually do?', [
    ['soon', 'Tell them soon and clearly', { conflictApproach: 0.9, communication: 0.85, assertiveness: 0.7, boundaries: 0.8, intimacyComfort: 0.6, trust: 0.7 }],
    ['wait', 'Wait until I have cooled down, then raise it', { conflictApproach: 0.65, communication: 0.7 }],
    ['avoid', 'Let it go and say nothing', { conflictApproach: 0.15, communication: 0.25, avoidance: 0.75, boundaries: 0.2, trust: 0.35 }],
    ['pull', 'Pull back for a while without explaining', { conflictApproach: 0.1, attachmentAvoidance: 0.7, communication: 0.15, intimacyComfort: 0.2 }],
  ]),
  choice('E_close', M, 'Someone you care about wants to know the real you. What do you usually do?', [
    ['open', 'Open up gradually and enjoy it', { feelingUnderstood: 0.8, relationshipQuality: 0.8, communication: 0.7, intimacyComfort: 0.9, attachmentAvoidance: 0.15, attachmentAnxiety: 0.2, trust: 0.8, companionshipNeed: 0.8 }],
    ['wary', 'Want it, but hold back until I feel safe', { intimacyComfort: 0.45, attachmentAnxiety: 0.7, trust: 0.4, companionshipNeed: 0.65 }],
    ['retreat', 'Keep things light and keep some distance', { intimacyComfort: 0.2, attachmentAvoidance: 0.85, trust: 0.3, relationalIndependence: 0.85, companionshipNeed: 0.25 }],
  ]),
  likert('E_lonely2', M, 'I have plenty of companionship in my life.', { loneliness: -1, relationshipQuality: 0.6, curRelationships: 0.6, relatedness: 0.5, feelingUnderstood: 0.4 }),
]
