import type { Question } from '../../engine/types'
import { anyOf, choice, forced, likert, multi, scale10, textQ } from '../helpers'

const M = 'F' as const

export const F: Question[] = [
  multi('F_acts', M, 'Which of these do you do, or have done regularly in the past few years? Pick up to eight.', 8, [
    ['sports', 'Team or competitive sports', { motCompetition: 0.85, motPhysical: 0.8, motSocial: 0.65 }],
    ['fitness', 'Fitness, running, lifting or yoga', { motPhysical: 0.85, motProgress: 0.7, motMastery: 0.6 }],
    ['outdoors', 'Hiking, camping or other outdoor time', { motNature: 0.9, motPhysical: 0.65, motEscapism: 0.6 }],
    ['music', 'Playing or making music', { motCreativity: 0.8, motMastery: 0.8, motProgress: 0.65 }],
    ['art', 'Drawing, painting, photography or design', { motCreativity: 0.9, motRelaxation: 0.6 }],
    ['writing', 'Writing', { motCreativity: 0.85, motMastery: 0.6, motEscapism: 0.5 }],
    ['making', 'Crafts, building or fixing things', { motCreativity: 0.7, motProblemSolving: 0.75, motProgress: 0.7 }],
    ['gaming', 'Video games', { motEscapism: 0.8, motProgress: 0.7, motCompetition: 0.6 }],
    ['boardgames', 'Board games, chess, puzzles', { motProblemSolving: 0.85, motCompetition: 0.65, motSocial: 0.6 }],
    ['reading', 'Reading', { motEscapism: 0.75, motRelaxation: 0.7 }],
    ['cooking', 'Cooking or baking', { motCreativity: 0.65, motRelaxation: 0.65, motSocial: 0.55 }],
    ['collecting', 'Collecting things', { motCollecting: 0.95, motProgress: 0.6 }],
    ['volunteering', 'Volunteering or community work', { motSocial: 0.75, contribution: 0.8 }],
    ['travel', 'Travelling and exploring places', { adventure: 0.8, motNature: 0.5, motEscapism: 0.6 }],
    ['tech', 'Coding, electronics or tinkering with tech', { motProblemSolving: 0.9, motMastery: 0.75, motCreativity: 0.55 }],
    ['clubs', 'Clubs, groups or social meetups', { motSocial: 0.9, motRecognition: 0.5 }],
    ['learning', 'Courses and learning new subjects', { motMastery: 0.8, motProgress: 0.75, motProblemSolving: 0.6 }],
    ['gardening', 'Gardening or caring for animals', { motNature: 0.8, motRelaxation: 0.8 }],
    ['performing', 'Performing, such as theatre, dance or public speaking', { motRecognition: 0.9, motCreativity: 0.65, motSocial: 0.6 }],
    ['screen', 'Films, series or streaming', { motEscapism: 0.75, motRelaxation: 0.8 }],
  ]),

  // Why do you enjoy it? Shown only for the activity families the person picked.
  multi(
    'F_why_move',
    M,
    'For sport and fitness, what do you enjoy most? Pick up to two.',
    2,
    [
      ['win', 'Competing and winning', { motCompetition: 0.95 }],
      ['better', 'Getting better over time', { motMastery: 0.85, motProgress: 0.9 }],
      ['team', 'Being with other people', { motSocial: 0.9 }],
      ['push', 'Pushing my body', { motPhysical: 0.95 }],
      ['clear', 'Clearing my head', { motRelaxation: 0.9, motEscapism: 0.5 }],
      ['air', 'Being outdoors', { motNature: 0.9 }],
    ],
    { showIf: anyOf('F_acts', ['sports', 'fitness']) },
  ),
  multi(
    'F_why_create',
    M,
    'For creative activities, what do you enjoy most? Pick up to two.',
    2,
    [
      ['new', 'Making something that did not exist before', { motCreativity: 0.95 }],
      ['skill', 'Getting more skilled', { motMastery: 0.9, motProgress: 0.8 }],
      ['seen', 'Having others see or appreciate it', { motRecognition: 0.9, motSocial: 0.5 }],
      ['switch', 'Switching off from everything else', { motRelaxation: 0.85, motEscapism: 0.8 }],
      ['solve', 'Working out how to make it work', { motProblemSolving: 0.85 }],
      ['share', 'Doing it with other people', { motSocial: 0.85 }],
    ],
    { showIf: anyOf('F_acts', ['music', 'art', 'writing', 'making', 'cooking']) },
  ),
  multi(
    'F_why_play',
    M,
    'For games and puzzles, what do you enjoy most? Pick up to two.',
    2,
    [
      ['comp', 'Beating other people', { motCompetition: 0.95 }],
      ['level', 'Levelling up or seeing my progress', { motProgress: 0.95, motMastery: 0.6 }],
      ['escape', 'Escaping into another world', { motEscapism: 0.95 }],
      ['puzzle', 'Solving hard problems', { motProblemSolving: 0.95 }],
      ['friends', 'Playing with friends', { motSocial: 0.9 }],
      ['collect', 'Unlocking or collecting things', { motCollecting: 0.9 }],
    ],
    { showIf: anyOf('F_acts', ['gaming', 'boardgames']) },
  ),
  multi(
    'F_why_mind',
    M,
    'For reading, learning and tech, what do you enjoy most? Pick up to two.',
    2,
    [
      ['solve', 'Figuring things out', { motProblemSolving: 0.95, intellectualDrive: 0.75, valKnowledge: 0.7 }],
      ['master', 'Becoming truly good at something', { motMastery: 0.95 }],
      ['progress', 'Seeing myself move forward', { motProgress: 0.95 }],
      ['escape', 'Getting lost in it', { motEscapism: 0.85 }],
      ['calm', 'Relaxing', { motRelaxation: 0.85 }],
      ['stories', 'Ideas and stories', { curiosity: 0.85, openness: 0.7, valKnowledge: 0.6 }],
    ],
    { showIf: anyOf('F_acts', ['reading', 'learning', 'tech', 'screen']) },
  ),

  likert('F_flow', M, 'There is at least one activity that absorbs me so fully that hours pass without my noticing.', { engagement: 1, motMastery: 0.3, curLeisure: 0.4, positiveEmotion: 0.4 }),
  forced(
    'F_solo',
    M,
    'For a free afternoon, which pulls you more?',
    ['group', 'Something with other people', { motSocial: 0.9, extraversion: 0.65, companionshipNeed: 0.7 }],
    ['solo', 'Something on my own', { motSocial: 0.15, relationalIndependence: 0.7, motRelaxation: 0.6 }],
  ),
  choice('F_sat', M, 'It is a completely free Saturday with no obligations. Which sounds best?', [
    ['trail', 'A long hike or a hard workout', { motPhysical: 0.9, motNature: 0.7, motEscapism: 0.4 }],
    ['studio', 'A project I have been wanting to make', { motCreativity: 0.9, motMastery: 0.6 }],
    ['game', 'A competitive game with friends', { motCompetition: 0.9, motSocial: 0.8 }],
    ['book', 'A book, show or game in a quiet corner', { motEscapism: 0.85, motRelaxation: 0.85 }],
    ['learn', 'Digging into something I want to master', { motMastery: 0.9, motProgress: 0.8, motProblemSolving: 0.6 }],
    ['out', 'Out exploring somewhere new', { adventure: 0.9, motNature: 0.6 }],
  ]),
  textQ('F_t1', M, 'Is there something you have always wanted to try but never have?', 'Optional. Anything counts.'),
]
