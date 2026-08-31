import { useEffect, useRef } from 'react'
import { useCommandPalette } from '../hooks/useCommandPalette'
import { useNavigate } from 'react-router-dom'
import type { CommandItem } from '../hooks/useCommandPalette'

export default function CommandPalette() {
  const navigate = useNavigate()
  const inputRef = useRef<HTMLInputElement>(null)

  const commands: CommandItem[] = [
    { id: 'dashboard', label: 'Go to Dashboard', shortcut: 'G D', action: () => navigate('/') },
    { id: 'create', label: 'Create VM', shortcut: 'G C', action: () => navigate('/create') },
    { id: 'settings', label: 'Settings', shortcut: 'G S', action: () => navigate('/settings') },
  ]

  const { isOpen, setIsOpen, query, setQuery, filtered } = useCommandPalette(commands)

  useEffect(() => {
    if (isOpen) {
      inputRef.current?.focus()
    }
  }, [isOpen])

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-[20vh]">
      <div className="fixed inset-0 bg-background/80 backdrop-blur-sm" onClick={() => setIsOpen(false)} />
      <div className="relative w-full max-w-lg bg-card border rounded-xl shadow-2xl overflow-hidden">
        <div className="p-3 border-b">
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="Type a command..."
            className="w-full px-3 py-2 bg-background border rounded-lg text-sm"
          />
        </div>
        <div className="max-h-80 overflow-auto p-2">
          {filtered.length === 0 && (
            <p className="p-3 text-sm text-muted-foreground text-center">No commands found</p>
          )}
          {filtered.map(cmd => (
            <button
              key={cmd.id}
              onClick={() => { cmd.action(); setIsOpen(false) }}
              className="w-full flex items-center justify-between px-3 py-2 rounded-lg hover:bg-muted text-left"
            >
              <span className="text-sm">{cmd.label}</span>
              {cmd.shortcut && <kbd className="text-xs px-2 py-0.5 bg-muted rounded">{cmd.shortcut}</kbd>}
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}
