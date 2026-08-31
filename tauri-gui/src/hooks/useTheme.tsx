import { useState, createContext, useContext, useEffect } from 'react'
import type { Theme } from '../lib/types'

interface ThemeContextValue {
  theme: Theme
  setTheme: (theme: Theme) => void
  resolved: 'light' | 'dark'
}

const ThemeContext = createContext<ThemeContextValue>({
  theme: 'system',
  setTheme: () => {},
  resolved: 'dark',
})

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(
    () => (localStorage.getItem('tridentd-theme') as Theme) || 'system'
  )
  const [resolved, setResolved] = useState<'light' | 'dark'>('dark')

  useEffect(() => {
    localStorage.setItem('tridentd-theme', theme)

    const update = () => {
      const isDark =
        theme === 'dark' ||
        (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
      setResolved(isDark ? 'dark' : 'light')
      document.documentElement.classList.toggle('dark', isDark)
    }

    update()
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    mq.addEventListener('change', update)
    return () => mq.removeEventListener('change', update)
  }, [theme])

  return (
    <ThemeContext.Provider value={{ theme, setTheme: setThemeState, resolved }}>
      {children}
    </ThemeContext.Provider>
  )
}

export const useTheme = () => useContext(ThemeContext)
