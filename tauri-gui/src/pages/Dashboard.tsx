import { useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import { Plus, Monitor, Cpu, MemoryStick, HardDrive, Loader2, AlertCircle, History } from 'lucide-react';
import { useState } from 'react';
import { useInstances } from '../hooks/useInstances';
import VmCard from '../components/VmCard';
import Modal from '../components/Modal';
import { api } from '../lib/api';
import { toast } from 'sonner';

export default function Dashboard() {
  const navigate = useNavigate();
  const { instances, loading, error, refresh } = useInstances();
  const [restoreOpen, setRestoreOpen] = useState(false);
  const [restoreId, setRestoreId] = useState('');
  const [restoreVcpu, setRestoreVcpu] = useState('');
  const [restoreMemory, setRestoreMemory] = useState('');
  const [restoring, setRestoring] = useState(false);

  const handleRestore = async () => {
    if (!restoreId.trim()) {
      toast.error('Enter a snapshot ID');
      return;
    }
    setRestoring(true);
    try {
      const info = await api.restoreSnapshot(
        restoreId.trim(),
        restoreVcpu ? parseInt(restoreVcpu) : undefined,
        restoreMemory ? parseInt(restoreMemory) : undefined,
      );
      toast.success(`Restored ${info.instance_id}`);
      setRestoreOpen(false);
      setRestoreId('');
      setRestoreVcpu('');
      setRestoreMemory('');
      refresh();
      navigate(`/vm/${info.instance_id}`);
    } catch (e) {
      toast.error(`Restore failed: ${e}`);
    } finally {
      setRestoring(false);
    }
  };

  const handleStop = async (id: string) => {
    try {
      await api.stopInstance(id);
      toast.success(`Instance ${id} stopped`);
      refresh();
    } catch (e) {
      toast.error(`Failed to stop: ${e}`);
    }
  };

  const handleFork = async (id: string) => {
    try {
      await api.forkInstance(id, 1);
      toast.success(`Forked ${id}`);
      refresh();
    } catch (e) {
      toast.error(`Failed to fork: ${e}`);
    }
  };

  const handlePing = async () => {
    try {
      const result = await api.ping();
      toast.success(`Daemon: ${result}`);
    } catch (e) {
      toast.error(`Daemon unreachable: ${e}`);
    }
  };

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
          <div className="flex gap-2">
            <button
              onClick={handlePing}
              className="px-4 py-2 bg-secondary rounded-lg hover:bg-secondary/80 transition-colors"
            >
              Ping Daemon
            </button>
            <button
              onClick={() => setRestoreOpen(true)}
              className="flex items-center gap-2 px-4 py-2 bg-secondary rounded-lg hover:bg-secondary/80 transition-colors"
            >
              <History className="w-4 h-4" />
              Restore Snapshot
            </button>
            <button
              onClick={() => navigate('/create')}
              className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
            >
              <Plus className="w-4 h-4" />
              Create VM
            </button>
          </div>
        </div>

        <Modal open={restoreOpen} title="Restore from Snapshot" onClose={() => setRestoreOpen(false)}>
          <div className="space-y-4">
            <div>
              <label className="block text-sm font-medium mb-2">Snapshot ID</label>
              <input
                type="text"
                value={restoreId}
                onChange={e => setRestoreId(e.target.value)}
                placeholder="snapshot ID from a previous Snapshot action"
                className="w-full px-3 py-2 bg-background border rounded-lg font-mono text-sm"
              />
            </div>
            <div className="grid grid-cols-2 gap-4">
              <div>
                <label className="block text-sm font-medium mb-2">vCPUs (optional)</label>
                <input
                  type="number"
                  min="1"
                  max="24"
                  value={restoreVcpu}
                  onChange={e => setRestoreVcpu(e.target.value)}
                  placeholder="unchanged"
                  className="w-full px-3 py-2 bg-background border rounded-lg text-sm"
                />
              </div>
              <div>
                <label className="block text-sm font-medium mb-2">Memory MiB (optional)</label>
                <input
                  type="number"
                  min="512"
                  step="512"
                  value={restoreMemory}
                  onChange={e => setRestoreMemory(e.target.value)}
                  placeholder="unchanged"
                  className="w-full px-3 py-2 bg-background border rounded-lg text-sm"
                />
              </div>
            </div>
            <button
              onClick={handleRestore}
              disabled={restoring}
              className="w-full py-2.5 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 disabled:opacity-50"
            >
              {restoring ? 'Restoring…' : 'Restore'}
            </button>
          </div>
        </Modal>

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
          
          {loading && (
            <div className="flex items-center justify-center py-12">
              <Loader2 className="w-8 h-8 animate-spin text-muted-foreground" />
              <span className="ml-2 text-muted-foreground">Loading instances...</span>
            </div>
          )}
          
          {error && (
            <div className="flex items-center justify-center py-12 text-destructive">
              <AlertCircle className="w-5 h-5 mr-2" />
              <span>Error: {error}</span>
            </div>
          )}
          
          {!loading && !error && instances.length === 0 && (
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
  );
}
