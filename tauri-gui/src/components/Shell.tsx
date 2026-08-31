import { Link, useLocation } from 'react-router-dom'
import { cn } from '../lib/utils'
import { 
  Monitor, 
  Plus, 
  Settings, 
  Terminal,
  Layers,
  Power
} from 'lucide-react'

const navItems = [
  { path: '/', icon: Monitor, label: 'Dashboard' },
  { path: '/create', icon: Plus, label: 'Create VM' },
  { path: '/settings', icon: Settings, label: 'Settings' },
]

export default function Shell({ children }: { children: React.ReactNode }) {
  const location = useLocation()

  return (
    <div className="flex h-screen bg-background">
      {/* Sidebar */}
      <aside className="w-64 border-r bg-card flex flex-col">
        <div className="p-4 border-b">
          <div className="flex items-center gap-2">
            <div className="w-8 h-8 rounded-lg bg-primary flex items-center justify-center">
              <Layers className="w-5 h-5 text-primary-foreground" />
            </div>
            <div>
              <h1 className="font-bold text-lg">TridentDroid</h1>
              <p className="text-xs text-muted-foreground">Android Emulator</p>
            </div>
          </div>
        </div>

        <nav className="flex-1 p-2">
          {navItems.map(item => {
            const isActive = location.pathname === item.path
            const Icon = item.icon
            return (
              <Link
                key={item.path}
                to={item.path}
                className={cn(
                  "flex items-center gap-3 px-3 py-2 rounded-lg mb-1 transition-colors",
                  isActive 
                    ? "bg-primary text-primary-foreground" 
                    : "hover:bg-muted text-muted-foreground hover:text-foreground"
                )}
              >
                <Icon className="w-5 h-5" />
                <span className="font-medium">{item.label}</span>
              </Link>
            )
          })}
        </nav>

        <div className="p-2 border-t">
          <div className="px-3 py-2 text-xs text-muted-foreground">
            <p>v0.1.0 • KVM/WHP</p>
          </div>
        </div>
      </aside>

      {/* Main content */}
      <main className="flex-1 overflow-auto">
        {children}
      </main>
    </div>
  )
}
