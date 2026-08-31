import { useEffect, useRef, useState } from 'react';
import { Maximize, Minimize, RefreshCw } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

interface DisplayProps {
  instanceId: string;
  width?: number;
  height?: number;
}

interface DisplayFrame {
  data: number[];
  pts_us: number;
  key_frame: boolean;
  width: number;
  height: number;
}

export default function Display({ instanceId, width = 1920, height = 1080 }: DisplayProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [fps, setFps] = useState(0);
  const [connected, setConnected] = useState(false);
  const frameCountRef = useRef(0);
  const lastFpsTimeRef = useRef(Date.now());

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Draw placeholder
    ctx.fillStyle = '#000000';
    ctx.fillRect(0, 0, width, height);
    
    ctx.fillStyle = '#666666';
    ctx.font = '24px sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText('Display Stream', width / 2, height / 2 - 20);
    ctx.font = '14px sans-serif';
    ctx.fillStyle = '#999999';
    ctx.fillText(`Instance: ${instanceId}`, width / 2, height / 2 + 10);
    ctx.fillText('Waiting for connection...', width / 2, height / 2 + 35);

    // Start the display stream
    invoke<void>('start_display_stream', { instanceId })
      .then(() => {
        setConnected(true);
      })
      .catch((e) => {
        console.error('Failed to start display stream:', e);
      });

    // Listen for display frame events
    const unlisten = listen<DisplayFrame>(`display_frame_${instanceId}`, (event) => {
      const frame = event.payload;
      if (!ctx || !canvas) return;

      // Update FPS counter
      frameCountRef.current++;
      const now = Date.now();
      if (now - lastFpsTimeRef.current >= 1000) {
        setFps(frameCountRef.current);
        frameCountRef.current = 0;
        lastFpsTimeRef.current = now;
      }

      // Render the frame
      if (frame.data && frame.data.length > 0) {
        // For raw RGBA data
        const imageData = new ImageData(
          new Uint8ClampedArray(frame.data),
          frame.width || width,
          frame.height || height
        );
        ctx.putImageData(imageData, 0, 0);
      }
    });

    return () => {
      unlisten.then((f) => f()).catch(() => {});
      invoke('close_display_stream', { instanceId }).catch(() => {});
    };
  }, [instanceId, width, height]);

  const toggleFullscreen = () => {
    setIsFullscreen(!isFullscreen);
  };

  return (
    <div className={`relative ${isFullscreen ? 'fixed inset-0 z-50 bg-black' : 'w-full'}`}>
      <div className="absolute top-2 right-2 z-10 flex gap-2">
        <button
          onClick={() => setConnected(!connected)}
          className="p-2 bg-background/80 backdrop-blur-sm rounded-lg hover:bg-background"
          title={connected ? 'Disconnect' : 'Connect'}
        >
          <RefreshCw className={`w-4 h-4 ${connected ? 'text-green-500' : 'text-muted-foreground'}`} />
        </button>
        <button
          onClick={toggleFullscreen}
          className="p-2 bg-background/80 backdrop-blur-sm rounded-lg hover:bg-background"
          title={isFullscreen ? 'Exit fullscreen' : 'Fullscreen'}
        >
          {isFullscreen ? <Minimize className="w-4 h-4" /> : <Maximize className="w-4 h-4" />}
        </button>
      </div>
      
      <canvas
        ref={canvasRef}
        width={width}
        height={height}
        className="w-full h-auto max-h-[70vh] object-contain bg-black rounded-lg"
      />
      
      <div className="absolute bottom-2 left-2 text-xs text-muted-foreground bg-background/80 backdrop-blur-sm px-2 py-1 rounded">
        {width}x{height} • {fps} FPS • {connected ? 'Connected' : 'Disconnected'}
      </div>
    </div>
  );
}
