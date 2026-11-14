import { useCallback, useEffect, useState } from 'react'
import { applyTheme, loadReport, loadTheme } from './engine/storage'
import type { Report } from './engine/types'
import { go, useRoute } from './lib/router'
import { Shell } from './components/ui'
import Landing from './pages/Landing'
import Intro from './pages/Intro'
import Questionnaire from './pages/Questionnaire'
import Results from './pages/Results'
import Explorer from './pages/Explorer'
import Try from './pages/Try'
import Simulator from './pages/Simulator'
import Contradictions from './pages/Contradictions'
import Growth from './pages/Growth'
import Share from './pages/Share'
import Settings from './pages/Settings'

export default function App() {
  const route = useRoute()
  const [report, setReport] = useState<Report | null>(() => loadReport())

  useEffect(() => {
    applyTheme(loadTheme())
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const on = () => loadTheme() === 'system' && applyTheme('system')
    mq.addEventListener('change', on)
    return () => mq.removeEventListener('change', on)
  }, [])

  const done = useCallback((r: Report) => {
    setReport(r)
    go('/results')
  }, [])
  const reset = useCallback(() => setReport(null), [])

  const [, first, rest] = route.split('/')
  const path = '/' + (first ?? '')

  switch (path) {
    case '/intro':
      return <Intro />
    case '/assess':
      return <Questionnaire onDone={done} />
    case '/settings':
      return (
        <Shell path={path}>
          <Settings onReset={reset} />
        </Shell>
      )
  }

  const needReport = ['/results', '/explore', '/try', '/simulator', '/contradictions', '/growth', '/share']
  if (!needReport.includes(path)) return <Landing report={report} />
  if (!report) {
    if (path === '/growth') return <Shell path={path}><Growth /></Shell>
    return <Landing report={null} />
  }

  return (
    <Shell path={path} wide={path === '/explore' || path === '/results' || path === '/simulator'}>
      {path === '/results' && <Results report={report} />}
      {path === '/explore' && <Explorer report={report} selected={rest} />}
      {path === '/try' && <Try report={report} />}
      {path === '/simulator' && <Simulator report={report} />}
      {path === '/contradictions' && <Contradictions report={report} />}
      {path === '/growth' && <Growth />}
      {path === '/share' && <Share report={report} />}
    </Shell>
  )
}
