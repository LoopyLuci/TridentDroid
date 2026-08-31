import { useState, useEffect, useCallback } from 'react'

export interface CommandItem {
  id: string
  label: string
  shortcut?: string
  action: () => void
}

export function useCommandPalette(commands: CommandItem[]) {
  const [isOpen, setIsOpen] = useState(false)
  const [query, setQuery] = useState('')

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        setIsOpen(prev => !prev)
        setQuery('')
      }
      if (e.key === 'Escape') {
        setIsOpen(false)
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [])

  const filtered = commands.filter(cmd =>
    cmd.label.toLowerCase().includes(query.toLowerCase())
  )

  return { isOpen, setIsOpen, query, setQuery, filtered }
}
