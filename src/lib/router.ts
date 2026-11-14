import { useEffect, useState } from 'react'

export function currentPath(): string {
  const h = window.location.hash.replace(/^#/, '')
  return h.startsWith('/') ? h : '/' + h
}
export function go(path: string) {
  if (currentPath() === path) {
    window.scrollTo(0, 0)
    return
  }
  window.location.hash = path
}
export function useRoute(): string {
  const [p, setP] = useState(currentPath())
  useEffect(() => {
    const on = () => {
      setP(currentPath())
      window.scrollTo(0, 0)
    }
    window.addEventListener('hashchange', on)
    return () => window.removeEventListener('hashchange', on)
  }, [])
  return p
}
