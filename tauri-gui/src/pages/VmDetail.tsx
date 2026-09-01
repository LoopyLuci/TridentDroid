import { useParams, useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import { ArrowLeft, Terminal, Monitor, Activity, Camera } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import TerminalComponent from '../components/Terminal';
import Display from '../components/Display';
import Modal from '../components/Modal';
import { api } from '../lib/api';

export default function VmDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [tab, setTab] = useState<'terminal' | 'display' | 'metrics'>('terminal');
  const [snapshotOpen, setSnapshotOpen] = useState(false);
  const [snapshotName, setSnapshotName] = useState('');
  const [snapshotting, setSnapshotting] = useState(false);

  const handleSnapshot = async () => {
    if (!id) return;
    setSnapshotting(true);
    try {
      // `include_disk` is a real proto field but the daemon doesn't act on
      // it yet (disk backing data is never captured, regardless) — not
      // surfaced as a UI toggle until that's actually implemented.
      const info = await api.createSnapshot(id, snapshotName.trim() || undefined, true);
      toast.success(`Snapshot saved: ${info.snapshot_id} (${(info.size_bytes / 1024 / 1024).toFixed(1)} MiB, ${info.duration_ms}ms)`);
      setSnapshotOpen(false);
      setSnapshotName('');
    } catch (e) {
      toast.error(`Snapshot failed: ${e}`);
    } finally {
      setSnapshotting(false);
    }
  };

  return (
    <div className="p-6 h-screen flex flex-col">
      <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="flex-1 flex flex-col min-h-0">
        <button onClick={() => navigate('/')} className="flex items-center gap-2 text-muted-foreground hover:text-foreground mb-4">
          <ArrowLeft className="w-4 h-4" /> Back
        </button>
        <div className="flex items-center justify-between mb-4">
          <h1 className="text-2xl font-bold">{id}</h1>
          <button
            onClick={() => setSnapshotOpen(true)}
            className="flex items-center gap-2 px-4 py-2 bg-secondary rounded-lg hover:bg-secondary/80 transition-colors"
          >
            <Camera className="w-4 h-4" /> Snapshot
          </button>
        </div>

        <Modal open={snapshotOpen} title="Create Snapshot" onClose={() => setSnapshotOpen(false)}>
          <div className="space-y-4">
            <div>
              <label className="block text-sm font-medium mb-2">Snapshot ID (optional)</label>
              <input
                type="text"
                value={snapshotName}
                onChange={e => setSnapshotName(e.target.value)}
                placeholder="leave empty to auto-generate"
                className="w-full px-3 py-2 bg-background border rounded-lg font-mono text-sm"
              />
            </div>
            <button
              onClick={handleSnapshot}
              disabled={snapshotting}
              className="w-full py-2.5 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 disabled:opacity-50"
            >
              {snapshotting ? 'Snapshotting…' : 'Create Snapshot'}
            </button>
          </div>
        </Modal>

        {/* Tabs */}
        <div className="flex gap-2 mb-4">
          <button
            onClick={() => setTab('terminal')}
            className={`flex items-center gap-2 px-4 py-2 rounded-lg ${tab === 'terminal' ? 'bg-primary text-primary-foreground' : 'bg-secondary'}`}
          >
            <Terminal className="w-4 h-4" /> Terminal
          </button>
          <button
            onClick={() => setTab('display')}
            className={`flex items-center gap-2 px-4 py-2 rounded-lg ${tab === 'display' ? 'bg-primary text-primary-foreground' : 'bg-secondary'}`}
          >
            <Monitor className="w-4 h-4" /> Display
          </button>
          <button
            onClick={() => setTab('metrics')}
            className={`flex items-center gap-2 px-4 py-2 rounded-lg ${tab === 'metrics' ? 'bg-primary text-primary-foreground' : 'bg-secondary'}`}
          >
            <Activity className="w-4 h-4" /> Metrics
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 min-h-0 bg-card border rounded-xl overflow-hidden">
          {tab === 'terminal' && (
            <div className="h-full p-2">
              <TerminalComponent instanceId={id || ''} />
            </div>
          )}
          {tab === 'display' && (
            <div className="h-full p-4 flex items-center justify-center">
              <Display instanceId={id || ''} />
            </div>
          )}
          {tab === 'metrics' && (
            <div className="p-6">
              <p className="text-muted-foreground">Metrics not available yet</p>
            </div>
          )}
        </div>
      </motion.div>
    </div>
  );
}
