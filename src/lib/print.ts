import { useSyncExternalStore } from 'react'

let printing = false
const subs = new Set<() => void>()
const set = (v: boolean) => {
  printing = v
  subs.forEach((f) => f())
}
if (typeof window !== 'undefined') {
  window.addEventListener('beforeprint', () => set(true))
  window.addEventListener('afterprint', () => set(false))
}
/** True while the browser is preparing a print or PDF export, so animations render in their final state. */
export function usePrinting(): boolean {
  return useSyncExternalStore(
    (f) => {
      subs.add(f)
      return () => subs.delete(f)
    },
    () => printing,
  )
}
