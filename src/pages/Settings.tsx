import { useState } from 'react'
import { applyTheme, clearProgress, deleteAll, exportAll, loadTheme, type Theme } from '../engine/storage'
import { Button, Card, PageTitle, cx } from '../components/ui'
import { go } from '../lib/router'

export default function Settings({ onReset }: { onReset: () => void }) {
  const [theme, setTheme] = useState<Theme>(loadTheme())
  const [confirm, setConfirm] = useState(false)

  const pick = (t: Theme) => {
    applyTheme(t)
    setTheme(t)
  }
  const download = () => {
    const blob = new Blob([exportAll()], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `human-index-${new Date().toISOString().slice(0, 10)}.json`
    document.body.appendChild(a)
    a.click()
    a.remove()
    URL.revokeObjectURL(url)
  }

  return (
    <div>
      <PageTitle eyebrow="Settings" title="Your data, your call" lead="Everything lives in this browser. There is no account and nothing is sent anywhere." />
      <div className="space-y-4">
        <Card>
          <h2 className="text-xl font-semibold">Appearance</h2>
          <div role="radiogroup" aria-label="Theme" className="mt-4 inline-flex rounded-full border border-line bg-raised p-1">
            {(['light', 'dark', 'system'] as Theme[]).map((t) => (
              <button key={t} type="button" role="radio" aria-checked={theme === t} onClick={() => pick(t)} className={cx('rounded-full px-5 py-1.5 text-[15px] capitalize', theme === t ? 'bg-ink text-bg' : 'text-muted hover:text-ink')}>{t}</button>
            ))}
          </div>
        </Card>
        <Card>
          <h2 className="text-xl font-semibold">Export my data</h2>
          <p className="mt-1 text-muted">Download your answers, results and history as a JSON file.</p>
          <div className="mt-4"><Button variant="secondary" onClick={download}>Download JSON</Button></div>
        </Card>
        <Card>
          <h2 className="text-xl font-semibold">Retake</h2>
          <p className="mt-1 text-muted">Start a fresh run. Your current results and past snapshots are kept until you finish, then a new snapshot is added.</p>
          <div className="mt-4"><Button variant="secondary" onClick={() => { clearProgress(); go('/intro') }}>Retake the questionnaire</Button></div>
        </Card>
        <Card>
          <h2 className="text-xl font-semibold">Delete everything</h2>
          <p className="mt-1 text-muted">Removes your answers, results, snapshots and simulator settings from this device. This cannot be undone.</p>
          <div className="mt-4">
            {!confirm ? (
              <Button variant="secondary" onClick={() => setConfirm(true)} className="!text-warn">Delete everything</Button>
            ) : (
              <div role="alertdialog" aria-label="Confirm delete" className="rounded-2xl border border-warn/40 bg-warn/5 p-4">
                <p className="font-medium">Delete all data on this device?</p>
                <div className="mt-3 flex gap-2">
                  <Button onClick={() => { deleteAll(); onReset(); go('/') }} className="!bg-warn">Yes, delete it all</Button>
                  <Button variant="ghost" onClick={() => setConfirm(false)}>Cancel</Button>
                </div>
              </div>
            )}
          </div>
        </Card>
      </div>
    </div>
  )
}
