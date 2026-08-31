import { motion } from 'framer-motion'
import { Play, Square, Copy, ExternalLink } from 'lucide-react'
import { cn } from '../lib/utils'
import type { InstanceInfo } from '../lib/types'

const stateColors: Record<string, string> = {
  booting: 'bg-yellow-500',
  running: 'bg-green-500',
  paused: 'bg-blue-500',
  stopped: 'bg-gray-500',
  faulted: 'bg-red-500',
}

export default function VmCard({ 
  instance, 
  onStop,
  onFork 
}: { 
  instance: InstanceInfo
  onStop: () => void
  onFork: () => void
}) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -20 }}
      className="bg-card border rounded-xl p-4 hover:border-primary/50 transition-colors"
    >
      <div className="flex items-start justify-between mb-3">
        <div className="flex items-center gap-3">
          <div className={cn("w-3 h-3 rounded-full", stateColors[instance.state] || 'bg-gray-500')} />
          <div>
            <h3 className="font-semibold">{instance.instance_id}</h3>
            <p className="text-xs text-muted-foreground capitalize">{instance.state}</p>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2 text-sm mb-3">
        <div>
          <p className="text-muted-foreground">ADB</p>
          <p className="font-mono text-xs">{instance.adb_host}:{instance.adb_port}</p>
        </div>
        <div>
          <p className="text-muted-foreground">Display</p>
          <p className="font-mono text-xs truncate">{instance.display_sock}</p>
        </div>
      </div>

      <div className="flex gap-2">
        <button
          onClick={onFork}
          className="flex items-center gap-1 px-3 py-1.5 text-xs bg-secondary hover:bg-secondary/80 rounded-lg transition-colors"
        >
          <Copy className="w-3 h-3" />
          Fork
        </button>
        <button
          onClick={onStop}
          className="flex items-center gap-1 px-3 py-1.5 text-xs bg-destructive text-destructive-foreground hover:bg-destructive/90 rounded-lg transition-colors"
        >
          <Square className="w-3 h-3" />
          Stop
        </button>
      </div>
    </motion.div>
  )
}
