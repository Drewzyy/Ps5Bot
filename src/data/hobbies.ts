// Hobby catalog. Each hobby carries weights (0..1) over the enjoyment-motive
// dimensions. These are hand-assigned judgements, not measured data, so treat
// the matching as a way to generate ideas, not a prediction.

import type { DimId } from '../engine/dimensions'

export type MotiveDim =
  | 'motCompetition'
  | 'motCreativity'
  | 'motMastery'
  | 'motSocial'
  | 'motCollecting'
  | 'motProblemSolving'
  | 'motRecognition'
  | 'motRelaxation'
  | 'motProgress'
  | 'motEscapism'
  | 'motPhysical'
  | 'motNature'

export const MOTIVE_DIMS: MotiveDim[] = [
  'motCompetition',
  'motCreativity',
  'motMastery',
  'motSocial',
  'motCollecting',
  'motProblemSolving',
  'motRecognition',
  'motRelaxation',
  'motProgress',
  'motEscapism',
  'motPhysical',
  'motNature',
]

export interface HobbyDef {
  id: string
  name: string
  motives: Partial<Record<MotiveDim, number>>
  /** Optional trait weights, 0..1 = how much a high score helps, negative = a low score helps. */
  traits?: Partial<Record<'openness' | 'extraversion' | 'realistic' | 'investigative' | 'artistic' | 'social' | 'enterprising' | 'conventional', number>>
  mightEnjoy: string
  mightDislike: string
  difficulty: 'Easy' | 'Moderate' | 'Demanding'
  cost: string
  firstStep: string
}

type Extra = Pick<HobbyDef, 'mightEnjoy' | 'mightDislike' | 'difficulty' | 'cost' | 'firstStep'> & { traits?: HobbyDef['traits'] }
const h = (id: string, name: string, motives: HobbyDef['motives'], x: Extra): HobbyDef => ({ id, name, motives, ...x })

export const HOBBIES: HobbyDef[] = [
  h('guitar', 'Learning guitar or another instrument', { motMastery: 1, motProgress: 0.9, motCreativity: 0.6, motRelaxation: 0.4, motRecognition: 0.2 }, {
    traits: { artistic: 0.5, openness: 0.3 },
    mightEnjoy: 'Measurable improvement week by week, and a skill that stays with you.',
    mightDislike: 'The awkward first two months, when practice sounds worse than it feels.',
    difficulty: 'Moderate', cost: '$0-150 to start',
    firstStep: 'Pick one simple song and practise 10 minutes a day for two weeks, with a free tutorial.',
  }),
  h('programming', 'Programming side projects', { motProblemSolving: 1, motMastery: 0.8, motProgress: 0.8, motCreativity: 0.6, motEscapism: 0.3 }, {
    traits: { investigative: 0.7, conventional: 0.2 },
    mightEnjoy: 'Puzzles that resolve into something working, and the pull of building your own tools.',
    mightDislike: 'Long stretches of debugging with nothing visible to show for it.',
    difficulty: 'Moderate', cost: '$0 to start',
    firstStep: 'Automate one small annoying task in your week and ship it in a weekend.',
  }),
  h('climbing', 'Rock climbing or bouldering', { motPhysical: 1, motProblemSolving: 0.8, motProgress: 0.8, motMastery: 0.7, motSocial: 0.5, motCompetition: 0.3 }, {
    traits: { realistic: 0.6, extraversion: 0.2 },
    mightEnjoy: 'Physical puzzles with clear grades, so progress is visible and friendly.',
    mightDislike: 'Skin soreness, plateaus, and heights if you are nervous about them.',
    difficulty: 'Moderate', cost: '$15-30 per session, shoes rentable',
    firstStep: 'Book an intro session at an indoor bouldering gym and go with one friend.',
  }),
  h('running', 'Running or training for a race', { motProgress: 1, motPhysical: 0.9, motCompetition: 0.5, motMastery: 0.4, motNature: 0.4, motEscapism: 0.3 }, {
    mightEnjoy: 'Numbers that improve, and a clean, simple structure of effort and recovery.',
    mightDislike: 'Early plateaus and injuries if you ramp up too fast.',
    difficulty: 'Moderate', cost: '$50-120 for shoes',
    firstStep: 'Follow a run/walk plan three times a week and sign up for a 5K in two months.',
  }),
  h('hiking', 'Hiking and trail exploration', { motNature: 1, motRelaxation: 0.7, motPhysical: 0.6, motEscapism: 0.6, motSocial: 0.3 }, {
    mightEnjoy: 'Quiet, scenery, and a reset that needs no performance.',
    mightDislike: 'Weather, crowds on popular trails, and logistics when planning alone.',
    difficulty: 'Easy', cost: '$0-100',
    firstStep: 'Pick a 2 hour trail within an hour of home and go this weekend.',
  }),
  h('painting', 'Painting or drawing', { motCreativity: 1, motRelaxation: 0.6, motMastery: 0.5, motEscapism: 0.5, motProgress: 0.3 }, {
    traits: { artistic: 0.8, openness: 0.5 },
    mightEnjoy: 'Making something that did not exist before, at your own pace.',
    mightDislike: 'Comparing early work to polished work online.',
    difficulty: 'Easy', cost: '$20-60',
    firstStep: 'Draw one small thing from life each day for 10 days, no erasing.',
  }),
  h('writing', 'Creative or reflective writing', { motCreativity: 0.9, motEscapism: 0.5, motMastery: 0.5, motRelaxation: 0.4, motRecognition: 0.3 }, {
    traits: { artistic: 0.8, openness: 0.5 },
    mightEnjoy: 'Working out what you think by putting it into words.',
    mightDislike: 'Blank-page pressure and slow external feedback.',
    difficulty: 'Easy', cost: '$0',
    firstStep: 'Write 200 words each morning for two weeks without editing.',
  }),
  h('photography', 'Photography', { motCreativity: 0.8, motCollecting: 0.5, motNature: 0.5, motMastery: 0.6, motProgress: 0.5 }, {
    traits: { artistic: 0.7 },
    mightEnjoy: 'Training your eye and returning with a concrete set of images.',
    mightDislike: 'Gear envy, which can swallow the actual shooting.',
    difficulty: 'Moderate', cost: '$0 with a phone, $300+ for a camera',
    firstStep: 'Choose one theme (light, doors, strangers\' hands) and take 20 photos on it this week.',
  }),
  h('chess', 'Chess or strategy board games', { motProblemSolving: 1, motCompetition: 0.8, motMastery: 0.8, motProgress: 0.6, motSocial: 0.3 }, {
    traits: { investigative: 0.6 },
    mightEnjoy: 'A clear rating that moves, and a problem that never runs out.',
    mightDislike: 'Losing streaks, which can sting if you tie results to ability.',
    difficulty: 'Moderate', cost: '$0',
    firstStep: 'Play five rated online games and review one loss with the engine.',
  }),
  h('boardgames', 'Hosting board game nights', { motSocial: 1, motCompetition: 0.5, motProblemSolving: 0.5, motCollecting: 0.4, motRelaxation: 0.3 }, {
    traits: { extraversion: 0.5, social: 0.4 },
    mightEnjoy: 'Structured fun that gives social time a shape.',
    mightDislike: 'Chasing people to turn up, and cooperating with very competitive friends.',
    difficulty: 'Easy', cost: '$20-60 per game',
    firstStep: 'Invite three people for a two-hour night with one easy game.',
  }),
  h('cooking', 'Cooking and recipe projects', { motCreativity: 0.7, motSocial: 0.5, motProgress: 0.5, motMastery: 0.6, motRelaxation: 0.5 }, {
    mightEnjoy: 'A tangible result within an hour, and people to share it with.',
    mightDislike: 'Weeknight fatigue turning it into a chore.',
    difficulty: 'Easy', cost: '$0-50 extra per week',
    firstStep: 'Pick one cuisine you do not cook and make three dishes from it over a month.',
  }),
  h('baking', 'Baking, especially sourdough or pastry', { motMastery: 0.8, motProgress: 0.7, motCreativity: 0.6, motRelaxation: 0.6, motSocial: 0.3 }, {
    traits: { conventional: 0.3 },
    mightEnjoy: 'A forgiving process with exact variables to tune.',
    mightDislike: 'Failed batches, and the temptation to eat the evidence.',
    difficulty: 'Moderate', cost: '$20-80',
    firstStep: 'Bake the same simple loaf three weekends running and change one variable each time.',
  }),
  h('gardening', 'Gardening or growing plants', { motNature: 0.9, motRelaxation: 0.8, motProgress: 0.6, motCollecting: 0.3, motMastery: 0.3 }, {
    mightEnjoy: 'Slow, visible growth and time away from screens.',
    mightDislike: 'Waiting on seasons, and losing plants to pests or neglect.',
    difficulty: 'Easy', cost: '$20-100',
    firstStep: 'Start three herbs on a windowsill or balcony and track them for a month.',
  }),
  h('yoga', 'Yoga or mobility practice', { motRelaxation: 0.9, motPhysical: 0.6, motProgress: 0.4, motEscapism: 0.4, motMastery: 0.3 }, {
    mightEnjoy: 'A calm structure that blends body and attention.',
    mightDislike: 'Slow pacing if you want intensity, or classes that feel performative.',
    difficulty: 'Easy', cost: '$0-15 per class',
    firstStep: 'Try three different classes or free videos and keep the one that feels least like a chore.',
  }),
  h('martialarts', 'Martial arts or boxing', { motPhysical: 1, motMastery: 0.8, motProgress: 0.8, motCompetition: 0.6, motSocial: 0.4 }, {
    traits: { realistic: 0.5 },
    mightEnjoy: 'Belts or ranks that mark progress, and an intense physical outlet.',
    mightDislike: 'Early clumsiness, and sparring if you dislike conflict.',
    difficulty: 'Demanding', cost: '$50-120 per month',
    firstStep: 'Take a free trial class at two local gyms and notice which culture suits you.',
  }),
  h('teamsport', 'Recreational team sport', { motSocial: 1, motCompetition: 0.8, motPhysical: 0.8, motProgress: 0.3 }, {
    traits: { extraversion: 0.7 },
    mightEnjoy: 'Shared goals, banter, and a standing weekly commitment.',
    mightDislike: 'Scheduling dependence, and teammates who take it very seriously or not at all.',
    difficulty: 'Easy', cost: '$30-150 per season',
    firstStep: 'Join a casual league that welcomes beginners and ask to be placed with newcomers.',
  }),
  h('swimming', 'Swimming laps or open-water swimming', { motPhysical: 0.8, motProgress: 0.7, motRelaxation: 0.6, motNature: 0.4, motEscapism: 0.4 }, {
    mightEnjoy: 'Rhythmic effort that quiets the mind, with clear split times.',
    mightDislike: 'Pool logistics, chlorine, and technique plateaus.',
    difficulty: 'Moderate', cost: '$5-10 per swim',
    firstStep: 'Swim twice a week for a month and log distance only.',
  }),
  h('cycling', 'Cycling or bikepacking', { motPhysical: 0.8, motNature: 0.7, motProgress: 0.6, motEscapism: 0.5, motMastery: 0.3, motSocial: 0.3 }, {
    traits: { realistic: 0.4 },
    mightEnjoy: 'Covering ground under your own power, alone or in a group.',
    mightDislike: 'Traffic, maintenance, and weather.',
    difficulty: 'Moderate', cost: '$200+ for a bike, less if second hand',
    firstStep: 'Plan a 30 km loop on quiet roads and bring snacks.',
  }),
  h('videogames', 'Story-rich or competitive video games', { motEscapism: 0.9, motCompetition: 0.6, motProblemSolving: 0.5, motSocial: 0.4, motProgress: 0.6, motCollecting: 0.4 }, {
    mightEnjoy: 'Immersion with tight feedback loops and clear goals.',
    mightDislike: 'Time that vanishes, which matters if it displaces things you value more.',
    difficulty: 'Easy', cost: '$0-60 per title',
    firstStep: 'Choose one game on purpose and set a weekly time budget before you start.',
  }),
  h('reading', 'Reading widely (fiction and nonfiction)', { motEscapism: 0.7, motRelaxation: 0.7, motCollecting: 0.3, motProgress: 0.3, motMastery: 0.3 }, {
    traits: { openness: 0.5 },
    mightEnjoy: 'Low-cost immersion and new angles on your own life.',
    mightDislike: 'Guilt about unfinished books.',
    difficulty: 'Easy', cost: '$0 with a library card',
    firstStep: 'Reserve three books from different genres and give each 30 pages before judging it.',
  }),
  h('bookclub', 'Book club or discussion group', { motSocial: 0.9, motProblemSolving: 0.4, motMastery: 0.3, motRecognition: 0.3, motRelaxation: 0.3 }, {
    traits: { extraversion: 0.4, openness: 0.3 },
    mightEnjoy: 'Reading with accountability and sharper conversations.',
    mightDislike: 'Assigned books that do not grab you.',
    difficulty: 'Easy', cost: '$0-20 per month',
    firstStep: 'Ask two friends to read the same short book and meet in four weeks.',
  }),
  h('languages', 'Learning a language', { motMastery: 0.9, motProgress: 0.9, motProblemSolving: 0.5, motSocial: 0.4, motCollecting: 0.3 }, {
    traits: { openness: 0.5 },
    mightEnjoy: 'Streaks and levels that make progress concrete, and real conversations later on.',
    mightDislike: 'The intermediate plateau where gains feel invisible.',
    difficulty: 'Demanding', cost: '$0-20 per month',
    firstStep: 'Do 15 minutes a day for 30 days and book one tutor chat to speak early.',
  }),
  h('woodworking', 'Woodworking or furniture building', { motCreativity: 0.7, motMastery: 0.9, motProgress: 0.7, motProblemSolving: 0.6, motRelaxation: 0.4 }, {
    traits: { realistic: 0.9 },
    mightEnjoy: 'Making durable objects with your hands and seeing exact improvement.',
    mightDislike: 'Tool costs, space needs, and unforgiving mistakes.',
    difficulty: 'Demanding', cost: '$100-500 for basic tools',
    firstStep: 'Build a simple shelf or box from a beginner plan with hand tools or a borrowed shop.',
  }),
  h('electronics', 'Electronics, robotics or Arduino tinkering', { motProblemSolving: 1, motCreativity: 0.6, motMastery: 0.7, motProgress: 0.6 }, {
    traits: { realistic: 0.7, investigative: 0.7 },
    mightEnjoy: 'Seeing a circuit you designed actually behave.',
    mightDislike: 'Debugging hardware where nothing is visibly wrong.',
    difficulty: 'Moderate', cost: '$30-80 starter kit',
    firstStep: 'Buy a starter kit and make an LED blink, then add a sensor.',
  }),
  h('diy', 'Home repair and DIY projects', { motProblemSolving: 0.7, motProgress: 0.8, motMastery: 0.6, motCreativity: 0.4 }, {
    traits: { realistic: 0.8 },
    mightEnjoy: 'Practical wins and a space that improves because of you.',
    mightDislike: 'Projects that sprawl, and tool trips.',
    difficulty: 'Moderate', cost: '$20-200 per project',
    firstStep: 'Fix the one thing that has annoyed you for months and film the process.',
  }),
  h('collecting', 'Collecting (vinyl, coins, books, plants, cards)', { motCollecting: 1, motRelaxation: 0.5, motMastery: 0.3, motSocial: 0.3 }, {
    traits: { conventional: 0.5 },
    mightEnjoy: 'Hunting, cataloguing and the satisfaction of a growing set.',
    mightDislike: 'Spending creep and clutter.',
    difficulty: 'Easy', cost: 'Variable, set a budget',
    firstStep: 'Choose a narrow category, set a monthly cap, and start a simple catalogue.',
  }),
  h('birdwatching', 'Birdwatching and wildlife spotting', { motNature: 1, motCollecting: 0.8, motRelaxation: 0.6, motProgress: 0.4, motMastery: 0.3 }, {
    mightEnjoy: 'A life list that grows, in quiet outdoor settings.',
    mightDislike: 'Early mornings and patience-heavy days.',
    difficulty: 'Easy', cost: '$0-100',
    firstStep: 'Install a free ID app and log every species you see for a week.',
  }),
  h('astronomy', 'Astronomy and stargazing', { motNature: 0.7, motProblemSolving: 0.4, motCollecting: 0.5, motMastery: 0.5, motEscapism: 0.5, motRelaxation: 0.4 }, {
    traits: { investigative: 0.6 },
    mightEnjoy: 'Quiet awe and learning how to find objects yourself.',
    mightDislike: 'Cloud cover, cold nights, and light pollution.',
    difficulty: 'Moderate', cost: '$0-300',
    firstStep: 'Use a sky app and binoculars to find Jupiter or the Moon\'s craters this week.',
  }),
  h('volunteering', 'Regular volunteering', { motSocial: 0.7, motProgress: 0.3, motRecognition: 0.2 }, {
    traits: { social: 0.8, extraversion: 0.2 },
    mightEnjoy: 'Visible usefulness and a community with a shared purpose.',
    mightDislike: 'Slow bureaucracy and emotional weight in some roles.',
    difficulty: 'Easy', cost: '$0',
    firstStep: 'Offer a recurring two-hour monthly slot to one local organisation and see how it feels.',
  }),
  h('mentoring', 'Mentoring or tutoring', { motSocial: 0.6, motMastery: 0.5, motRecognition: 0.3, motProgress: 0.4 }, {
    traits: { social: 0.9 },
    mightEnjoy: 'Watching someone grow because of what you shared.',
    mightDislike: 'Mismatched expectations or inconsistent mentees.',
    difficulty: 'Moderate', cost: '$0',
    firstStep: 'Offer to help one junior colleague or a local student with a specific skill for six sessions.',
  }),
  h('theatre', 'Improv or community theatre', { motSocial: 0.8, motCreativity: 0.8, motRecognition: 0.7, motEscapism: 0.4, motMastery: 0.3 }, {
    traits: { extraversion: 0.8, artistic: 0.6 },
    mightEnjoy: 'Play, risk and an audience, in a supportive group.',
    mightDislike: 'Being watched, if exposure feels heavy.',
    difficulty: 'Moderate', cost: '$100-200 per course',
    firstStep: 'Sign up for a beginner improv drop-in and take the pressure off by watching one first.',
  }),
  h('choir', 'Singing in a choir or band', { motSocial: 0.8, motCreativity: 0.6, motMastery: 0.5, motRecognition: 0.4, motRelaxation: 0.3 }, {
    traits: { artistic: 0.5, extraversion: 0.4 },
    mightEnjoy: 'Blended sound and belonging, with a shared rehearsal rhythm.',
    mightDislike: 'Rehearsal commitment and performance nerves.',
    difficulty: 'Moderate', cost: '$0-100 per season',
    firstStep: 'Attend a trial rehearsal of a community choir that has no audition.',
  }),
  h('dance', 'Dance (salsa, swing, contemporary)', { motPhysical: 0.8, motSocial: 0.8, motCreativity: 0.5, motMastery: 0.6, motRecognition: 0.3 }, {
    traits: { extraversion: 0.6 },
    mightEnjoy: 'Movement with other people and quick visible progress.',
    mightDislike: 'Self-consciousness at the start.',
    difficulty: 'Moderate', cost: '$10-20 per class',
    firstStep: 'Go to a beginner class that rotates partners and commit to three sessions.',
  }),
  h('poker', 'Poker or competitive card games', { motCompetition: 1, motProblemSolving: 0.7, motSocial: 0.5, motMastery: 0.5 }, {
    traits: { enterprising: 0.5 },
    mightEnjoy: 'Decision quality under uncertainty and friendly rivalry.',
    mightDislike: 'Variance, and the pull of money stakes. Keep it low stakes.',
    difficulty: 'Moderate', cost: '$0-20 per night',
    firstStep: 'Host a no-money home game and read one beginner strategy article first.',
  }),
  h('esports', 'Competitive esports or ladder play', { motCompetition: 1, motMastery: 0.8, motProgress: 0.8, motRecognition: 0.5, motSocial: 0.4 }, {
    mightEnjoy: 'Ranked progress and tight team coordination.',
    mightDislike: 'Toxic lobbies, and the risk of tilt.',
    difficulty: 'Demanding', cost: '$0-60',
    firstStep: 'Pick one game, review one replay per session, and set a weekly cap.',
  }),
  h('blogging', 'Blogging, podcasting or video making', { motCreativity: 0.8, motRecognition: 0.8, motProgress: 0.6, motSocial: 0.3, motMastery: 0.4 }, {
    traits: { artistic: 0.4, enterprising: 0.4 },
    mightEnjoy: 'Putting your voice out and watching an audience form.',
    mightDislike: 'Slow early growth and feedback that can feel personal.',
    difficulty: 'Moderate', cost: '$0-100',
    firstStep: 'Publish five short pieces in five weeks on one niche, and ignore the stats until then.',
  }),
  h('music-prod', 'Music production or DJing', { motCreativity: 0.9, motProblemSolving: 0.5, motMastery: 0.7, motProgress: 0.6, motRecognition: 0.4 }, {
    traits: { artistic: 0.7, realistic: 0.3 },
    mightEnjoy: 'Building sound from nothing and iterating quickly.',
    mightDislike: 'Endless tweaking without finishing tracks.',
    difficulty: 'Moderate', cost: '$0-300',
    firstStep: 'Finish one 16-bar loop in a free tool within a week, even if it is rough.',
  }),
  h('pottery', 'Pottery or ceramics', { motCreativity: 0.8, motRelaxation: 0.7, motMastery: 0.6, motProgress: 0.5, motSocial: 0.3 }, {
    traits: { artistic: 0.7, realistic: 0.5 },
    mightEnjoy: 'Absorbing hand work with a physical result you can use.',
    mightDislike: 'Lost pieces in the kiln and slow feedback.',
    difficulty: 'Moderate', cost: '$150-300 for a course',
    firstStep: 'Take a one-off wheel throwing taster class.',
  }),
  h('knitting', 'Knitting, crochet or sewing', { motCreativity: 0.7, motRelaxation: 0.8, motProgress: 0.7, motMastery: 0.5, motCollecting: 0.3 }, {
    mightEnjoy: 'Meditative repetition with each row showing progress.',
    mightDislike: 'Pattern errors found many rows later.',
    difficulty: 'Easy', cost: '$15-40',
    firstStep: 'Knit a simple scarf with a beginner video, one hour per week.',
  }),
  h('travel', 'Solo or small-group travel planning', { motEscapism: 0.7, motNature: 0.4, motCollecting: 0.4, motSocial: 0.3, motProblemSolving: 0.3 }, {
    traits: { openness: 0.7 },
    mightEnjoy: 'Novelty, and the planning is half the pleasure.',
    mightDislike: 'Cost, and the post-trip drop.',
    difficulty: 'Moderate', cost: '$100+ per trip',
    firstStep: 'Plan a cheap two-day trip somewhere you have never been within three hours of home.',
  }),
  h('camping', 'Camping and backpacking', { motNature: 1, motPhysical: 0.6, motEscapism: 0.7, motProblemSolving: 0.4, motSocial: 0.3 }, {
    traits: { realistic: 0.6 },
    mightEnjoy: 'Self-reliance and unplugged time in the outdoors.',
    mightDislike: 'Gear investment, bugs, and bad weather.',
    difficulty: 'Moderate', cost: '$100-400 for gear',
    firstStep: 'Borrow a tent and do one overnight at a car-accessible site.',
  }),
  h('fishing', 'Fishing', { motNature: 0.9, motRelaxation: 0.8, motProgress: 0.3, motCollecting: 0.3, motEscapism: 0.5 }, {
    mightEnjoy: 'Patience, quiet, and being by the water.',
    mightDislike: 'Slow days with no catch.',
    difficulty: 'Easy', cost: '$30-150',
    firstStep: 'Get a local licence and spend one morning with a friend who fishes.',
  }),
  h('sailing', 'Sailing or paddle sports', { motNature: 0.8, motPhysical: 0.6, motMastery: 0.7, motProblemSolving: 0.5, motSocial: 0.4 }, {
    traits: { realistic: 0.6 },
    mightEnjoy: 'Reading wind and water, and skills that compound.',
    mightDislike: 'Cost and weather dependence.',
    difficulty: 'Demanding', cost: '$100-400 for lessons',
    firstStep: 'Book a beginner kayak or dinghy lesson and ask about a club membership.',
  }),
  h('strength', 'Strength training', { motProgress: 1, motPhysical: 0.8, motMastery: 0.6, motCompetition: 0.3, motRelaxation: 0.2 }, {
    traits: { conventional: 0.3 },
    mightEnjoy: 'Numbers that rise reliably and a simple, repeatable routine.',
    mightDislike: 'Plateaus and comparing yourself to others at the gym.',
    difficulty: 'Moderate', cost: '$20-50 per month',
    firstStep: 'Follow a three-day beginner programme and log every set for 8 weeks.',
  }),
  h('meditation', 'Meditation and mindfulness practice', { motRelaxation: 1, motMastery: 0.3, motProgress: 0.3, motEscapism: 0.3 }, {
    mightEnjoy: 'Quiet, and noticing thoughts without acting on each one.',
    mightDislike: 'Restlessness if you crave external stimulation.',
    difficulty: 'Easy', cost: '$0',
    firstStep: 'Sit for five minutes after waking, using a free timer, for 14 days.',
  }),
  h('journaling', 'Journaling and life review', { motCreativity: 0.3, motRelaxation: 0.6, motProgress: 0.5, motMastery: 0.3, motEscapism: 0.2 }, {
    traits: { openness: 0.4 },
    mightEnjoy: 'Seeing patterns in your own thinking over time.',
    mightDislike: 'Feeling you should be writing perfectly.',
    difficulty: 'Easy', cost: '$0-15',
    firstStep: 'Write three lines each evening: what happened, what mattered, what you want tomorrow.',
  }),
  h('startup', 'Starting a small side business', { motProgress: 0.8, motProblemSolving: 0.7, motRecognition: 0.6, motCompetition: 0.6, motCreativity: 0.5, motMastery: 0.5 }, {
    traits: { enterprising: 0.9 },
    mightEnjoy: 'Building something yours, with market feedback.',
    mightDislike: 'Uncertain income and the pressure of making every hour count.',
    difficulty: 'Demanding', cost: '$50-500',
    firstStep: 'Sell one simple product or service to one real customer before building anything else.',
  }),
  h('investing', 'Investing and personal finance', { motProblemSolving: 0.7, motProgress: 0.7, motCompetition: 0.4, motMastery: 0.5, motCollecting: 0.3 }, {
    traits: { conventional: 0.5, investigative: 0.5 },
    mightEnjoy: 'Long-horizon systems and compounding numbers.',
    mightDislike: 'Market swings and the temptation to over-trade.',
    difficulty: 'Moderate', cost: '$0 to learn',
    firstStep: 'Read one index-investing primer, then track your own spending for a month.',
  }),
  h('genealogy', 'Genealogy and local history', { motCollecting: 0.8, motProblemSolving: 0.7, motEscapism: 0.4, motProgress: 0.4 }, {
    traits: { investigative: 0.6, conventional: 0.4 },
    mightEnjoy: 'Detective work that links you to real people.',
    mightDislike: 'Dead ends in thin records.',
    difficulty: 'Moderate', cost: '$0-100',
    firstStep: 'Interview one relative and chart three generations.',
  }),
  h('puzzles', 'Jigsaws, crosswords and logic puzzles', { motProblemSolving: 0.9, motRelaxation: 0.7, motProgress: 0.5, motCompetition: 0.2 }, {
    traits: { investigative: 0.4, conventional: 0.3 },
    mightEnjoy: 'Calm focus with a clear finish.',
    mightDislike: 'Getting stuck with no hints.',
    difficulty: 'Easy', cost: '$0-30',
    firstStep: 'Do one daily puzzle for two weeks and track your time.',
  }),
  h('trivia', 'Pub quiz or trivia league', { motCompetition: 0.8, motSocial: 0.9, motRecognition: 0.4, motProblemSolving: 0.3 }, {
    traits: { extraversion: 0.5 },
    mightEnjoy: 'Friendly competition with banter.',
    mightDislike: 'Late nights and noisy rooms.',
    difficulty: 'Easy', cost: '$5-15',
    firstStep: 'Join a weekly pub quiz with two friends.',
  }),
  h('debate', 'Debate, Toastmasters or public speaking', { motCompetition: 0.7, motRecognition: 0.8, motMastery: 0.7, motProgress: 0.6, motSocial: 0.5 }, {
    traits: { enterprising: 0.7, extraversion: 0.5 },
    mightEnjoy: 'Measurable progress in a skill that transfers to work.',
    mightDislike: 'Early nerves in front of an audience.',
    difficulty: 'Moderate', cost: '$0-100',
    firstStep: 'Visit a local Toastmasters meeting as a guest and watch one session.',
  }),
  h('trailrun', 'Trail running or ultra-distance events', { motPhysical: 1, motNature: 0.8, motProgress: 0.8, motCompetition: 0.5, motEscapism: 0.5 }, {
    mightEnjoy: 'Long days in nature with big, trackable goals.',
    mightDislike: 'Injury risk and heavy training load.',
    difficulty: 'Demanding', cost: '$100-250',
    firstStep: 'Run a local 10K trail event, then decide whether you want longer.',
  }),
  h('3dprint', '3D printing and maker projects', { motCreativity: 0.8, motProblemSolving: 0.8, motMastery: 0.6, motProgress: 0.6 }, {
    traits: { realistic: 0.7, investigative: 0.4 },
    mightEnjoy: 'Designing and holding the result a few hours later.',
    mightDislike: 'Calibration headaches and failed prints.',
    difficulty: 'Moderate', cost: '$200-400',
    firstStep: 'Use a library or makerspace printer to print one useful object before buying a machine.',
  }),
  h('animals', 'Dog training or animal fostering', { motSocial: 0.5, motNature: 0.6, motProgress: 0.6, motMastery: 0.5, motRelaxation: 0.4 }, {
    traits: { social: 0.5 },
    mightEnjoy: 'A warm relationship plus daily routine and outdoor time.',
    mightDislike: 'Responsibility and cost, and the heartbreak of fostering goodbyes.',
    difficulty: 'Moderate', cost: '$50-300',
    firstStep: 'Volunteer one weekend at a shelter before committing to an animal.',
  }),
  h('community', 'Community organising or local clubs', { motSocial: 0.8, motRecognition: 0.4, motProblemSolving: 0.5, motProgress: 0.5 }, {
    traits: { enterprising: 0.5, social: 0.6, extraversion: 0.4 },
    mightEnjoy: 'Seeing a group form around something you started.',
    mightDislike: 'Meetings and unpaid administration.',
    difficulty: 'Moderate', cost: '$0-50',
    firstStep: 'Start a monthly meetup on one specific interest in a free room or park.',
  }),
]
export type { DimId }
