import { useEffect, useState } from 'react'

const names = ['accent', 'ink', 'muted', 'faint', 'line', 'good', 'warn', 'surface', 'bg'] as const
export type Palette = Record<(typeof names)[number], string>

function read(): Palette {
  const cs = getComputedStyle(document.documentElement)
  const out = {} as Palette
  for (const n of names) {
    const v = cs.getPropertyValue(`--${n}`).trim().split(/\s+/).join(',')
    out[n] = `rgb(${v})`
  }
  return out
}

/** Theme colours resolved to concrete values, for SVG and chart libraries. */
export function useColors(): Palette {
  const [c, setC] = useState<Palette>(read)
  useEffect(() => {
    const mo = new MutationObserver(() => setC(read()))
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
    return () => mo.disconnect()
  }, [])
  return c
}
