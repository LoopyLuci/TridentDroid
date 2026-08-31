import { useNavigate } from 'react-router-dom'
import { motion } from 'framer-motion'
import { Plus, Monitor, Cpu, MemoryStick, HardDrive } from 'lucide-react'
import { useInstances } from '../hooks/useInstances'
import VmCard from '../components/VmCard'
import { api } from '../lib/api'
import { toast } from 'sonner'

export default function Dashboard() {
  const navigate = useNavigate()
  const { instances, loading, error, refresh } = useInstances()

  const handleStop = async (id: string) => {
    try {
      await api.stopInstance(id)
      toast.success(`Instance ${id} stopped`)
      refresh()
    } catch (e) {
      toast.error(`Failed to stop: ${e}`)
    }
  }

  const handleFork = async (id: string) => {
    try {
      await api.forkInstance(id, 1)
      toast.success(`Forked ${id}`)
      refresh()
    } catch (e) {
      toast.error(`Failed to fork: ${e}`)
    }
  }

  return (
    <div className="p-6 max-w-7xl mx-auto">
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        className="mb-6"
      >
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-3xl font-bold">Dashboard</h1>
            <p className="text-muted-foreground">Manage your Android emulator instances</p>
          </div>
          <button
            onClick={() => navigate('/create')}
            className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
          >
            <Plus className="w-4 h-4" />
            Create VM
          </button>
        </div>

        {/* Stats */}
        <div className="grid grid-cols-4 gap-4 mb-6">
          <div className="bg-card border rounded-xl p-4">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-primary/10 rounded-lg">
                <Monitor className="w-5 h-5 text-primary" />
              </div>
              <div>
                <p className="text-2xl font-bold">{instances.length}</p>
                <p className="text-xs text-muted-foreground">Instances</p>
              </div>
            </div>
          </div>
          <div className="bg-card border rounded-xl p-4">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-green-500/10 rounded-lg">
                <Cpu className="w-5 h-5 text-green-500" />
              </div>
              <div>
                <p className="text-2xl font-bold">4</p>
                <p className="text-xs text-muted-foreground">vCPUs</p>
              </div>
            </div>
          </div>
          <div className="bg-card border rounded-xl p-4">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-blue-500/10 rounded-lg">
                <MemoryStick className="w-5 h-5 text-blue-500" />
              </div>
              <div>
                <p className="text-2xl font-bold">8 GB</p>
                <p className="text-xs text-muted-foreground">Memory</p>
              </div>
            </div>
          </div>
          <div className="bg-card border rounded-xl p-4">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-purple-500/10 rounded-lg">
                <HardDrive className="w-5 h-5 text-purple-500" />
              </div>
              <div>
                <p className="text-2xl font-bold">2</p>
                <p className="text-xs text-muted-foreground">Disk Images</p>
              </div>
            </div>
          </div>
        </div>

        {/* Instance list */}
        <div>
          <h2 className="text-xl font-semibold mb-4">Instances</h2>
          {loading && <p className="text-muted-foreground">Loading...</p>}
          {error && <p className="text-destructive">Error: {error}</p>}
          {!loading && instances.length === 0 && (
            <div className="text-center py-12 bg-card border rounded-xl">
              <Monitor className="w-12 h-12 mx-auto text-muted-foreground mb-4" />
              <p className="text-muted-foreground mb-4">No instances yet</p>
              <button
                onClick={() => navigate('/create')}
                className="px-4 py-2 bg-primary text-primary-foreground rounded-lg"
              >
                Create your first VM
              </button>
            </div>
          )}
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {instances.map(instance => (
              <VmCard
                key={instance.instance_id}
                instance={instance}
                onStop={() => handleStop(instance.instance_id)}
                onFork={() => handleFork(instance.instance_id)}
              />
            ))}
          </div>
        </div>
      </motion.div>
    </div>
  )
}
