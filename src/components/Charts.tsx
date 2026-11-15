import { Bar, BarChart, CartesianGrid, Cell, LabelList, Legend, Line, LineChart, PolarAngleAxis, PolarGrid, PolarRadiusAxis, Radar, RadarChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { useColors } from './useColors'
import { usePrinting } from '../lib/print'

export interface Pt {
  label: string
  value: number
}

const trunc = (s: string, n = 28) => (s.length > n ? s.slice(0, n - 1) + '…' : s)

function useTip() {
  const c = useColors()
  return {
    contentStyle: { background: c.surface, border: `1px solid ${c.line}`, borderRadius: 12, color: c.ink, fontSize: 13 },
    labelStyle: { color: c.ink, fontWeight: 600 },
    itemStyle: { color: c.muted },
    cursor: { fill: c.line, fillOpacity: 0.35 },
  }
}

export function RadarBlock({ data, height = 300, max = 100, name = 'Score' }: { data: Pt[]; height?: number; max?: number; name?: string }) {
  const c = useColors()
  const tip = useTip()
  const pr = usePrinting()
  return (
    <div style={{ height }} role="img" aria-label={`Radar chart: ${data.map((d) => `${d.label} ${Math.round(d.value)}`).join(', ')}`}>
      <ResponsiveContainer width="100%" height="100%">
        <RadarChart data={data} outerRadius="68%">
          <PolarGrid stroke={c.line} />
          <PolarAngleAxis dataKey="label" tick={{ fill: c.muted, fontSize: 12 }} />
          <PolarRadiusAxis domain={[0, max]} tick={false} axisLine={false} />
          <Radar name={name} dataKey="value" stroke={c.accent} fill={c.accent} fillOpacity={0.28} strokeWidth={2} isAnimationActive={!pr} />
          <Tooltip {...tip} />
        </RadarChart>
      </ResponsiveContainer>
    </div>
  )
}

export function HBars({ data, height, max = 100, colorFn, name = 'Score', labelWidth = 136 }: { data: Pt[]; height?: number; max?: number; colorFn?: (p: Pt, i: number) => string; name?: string; labelWidth?: number }) {
  const c = useColors()
  const tip = useTip()
  const pr = usePrinting()
  const h = height ?? Math.max(140, data.length * 34 + 20)
  const rows = data.map((d) => ({ ...d, value: Math.round(d.value * 10) / 10, short: trunc(d.label) }))
  return (
    <div style={{ height: h }} role="img" aria-label={`Bar chart: ${data.map((d) => `${d.label} ${Math.round(d.value)}`).join(', ')}`}>
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={rows} layout="vertical" margin={{ left: 0, right: 28, top: 4, bottom: 4 }} barCategoryGap={8}>
          <XAxis type="number" domain={[0, max]} hide />
          <YAxis type="category" dataKey="short" width={labelWidth} tick={{ fill: c.muted, fontSize: 12 }} axisLine={false} tickLine={false} interval={0} />
          <Tooltip {...tip} formatter={(v) => [v as number, name]} labelFormatter={(_, p) => (p?.[0]?.payload?.label as string) ?? ''} />
          <Bar dataKey="value" radius={[0, 6, 6, 0]} isAnimationActive={!pr} barSize={16}>
            {rows.map((r, i) => (
              <Cell key={r.label} fill={colorFn ? colorFn(r, i) : c.accent} fillOpacity={0.9} />
            ))}
            <LabelList dataKey="value" position="right" fill={c.muted} fontSize={12} formatter={(v: number) => Math.round(v)} />
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </div>
  )
}

export function GroupedBars({ data, a, b, height = 300 }: { data: { label: string; a: number; b: number }[]; a: string; b: string; height?: number }) {
  const c = useColors()
  const tip = useTip()
  const pr = usePrinting()
  return (
    <div style={{ height }} role="img" aria-label={`Chart comparing ${a} and ${b}: ${data.map((d) => `${d.label} ${Math.round(d.a)} and ${Math.round(d.b)}`).join(', ')}`}>
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={data} margin={{ left: -18, right: 4, top: 8, bottom: 4 }}>
          <CartesianGrid stroke={c.line} vertical={false} />
          <XAxis dataKey="label" tick={{ fill: c.muted, fontSize: 12 }} axisLine={false} tickLine={false} interval={0} />
          <YAxis domain={[0, 100]} tick={{ fill: c.faint, fontSize: 11 }} axisLine={false} tickLine={false} />
          <Tooltip {...tip} />
          <Legend wrapperStyle={{ fontSize: 13, color: c.muted }} />
          <Bar dataKey="a" name={a} fill={c.faint} radius={[6, 6, 0, 0]} isAnimationActive={!pr} />
          <Bar dataKey="b" name={b} fill={c.accent} radius={[6, 6, 0, 0]} isAnimationActive={!pr} />
        </BarChart>
      </ResponsiveContainer>
    </div>
  )
}

export interface Series {
  key: string
  name: string
  color: string
}
export function Lines({ data, series, height = 320 }: { data: Record<string, number | string>[]; series: Series[]; height?: number }) {
  const c = useColors()
  const tip = useTip()
  const pr = usePrinting()
  return (
    <div style={{ height }} role="img" aria-label="Line chart of your scores across snapshots">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data} margin={{ left: -18, right: 12, top: 8, bottom: 4 }}>
          <CartesianGrid stroke={c.line} vertical={false} />
          <XAxis dataKey="date" tick={{ fill: c.muted, fontSize: 12 }} axisLine={false} tickLine={false} />
          <YAxis domain={[0, 100]} tick={{ fill: c.faint, fontSize: 11 }} axisLine={false} tickLine={false} />
          <Tooltip {...tip} />
          <Legend wrapperStyle={{ fontSize: 13, color: c.muted }} />
          {series.map((s) => (
            <Line key={s.key} type="monotone" dataKey={s.key} name={s.name} stroke={s.color} strokeWidth={2} dot={{ r: 3 }} isAnimationActive={!pr} />
          ))}
        </LineChart>
      </ResponsiveContainer>
    </div>
  )
}
