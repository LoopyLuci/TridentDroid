import { useEffect, useRef, useState } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

interface TerminalProps {
  instanceId: string;
}

export default function Terminal({ instanceId }: TerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const [connected, setConnected] = useState(false);
  const commandBufferRef = useRef('');

  useEffect(() => {
    if (!terminalRef.current) return;

    const term = new XTerm({
      theme: {
        background: '#0a0a0a',
        foreground: '#d4d4d4',
        cursor: '#ffffff',
        selectionBackground: '#264f78',
        black: '#0a0a0a',
        red: '#cd3131',
        green: '#0dbc79',
        yellow: '#e5e510',
        blue: '#2472c8',
        magenta: '#bc3fbc',
        cyan: '#11a8cd',
        white: '#e5e5e5',
      },
      fontFamily: 'JetBrains Mono, Fira Code, monospace',
      fontSize: 13,
      lineHeight: 1.2,
      cursorBlink: true,
      cursorStyle: 'block',
      scrollback: 10000,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(terminalRef.current);
    fitAddon.fit();

    xtermRef.current = term;
    fitAddonRef.current = fitAddon;

    term.writeln('\x1b[1;34mTridentDroid ADB Shell\x1b[0m');
    term.writeln(`\x1b[90mInstance: ${instanceId}\x1b[0m`);
    term.writeln('\x1b[90mConnecting...\x1b[0m');
    term.writeln('');

    // Start the ADB shell session
    invoke<void>('start_adb_shell', { instanceId })
      .then(() => {
        setConnected(true);
        term.writeln('\x1b[1;32mConnected!\x1b[0m');
        term.writeln('');
        term.write('$ ');
      })
      .catch((e) => {
        term.writeln(`\x1b[1;31mConnection failed: ${e}\x1b[0m`);
        term.writeln('\x1b[90mType commands to retry...\x1b[0m');
      });

    // Listen for shell output events
    const unlisten = listen<string>(`adb_shell_${instanceId}`, (event) => {
      if (xtermRef.current) {
        xtermRef.current.write(event.payload);
      }
    });

    const executeCommand = async (cmd: string) => {
      try {
        await invoke<void>('send_adb_command', { instanceId, command: cmd });
      } catch (e) {
        term.writeln(`\x1b[1;31mError: ${e}\x1b[0m`);
        term.write('$ ');
      }
    };

    term.onKey((e: { key: string; domEvent: KeyboardEvent }) => {
      const { key, domEvent } = e;
      const printable = !domEvent.altKey && !domEvent.ctrlKey && !domEvent.metaKey;
      
      if (domEvent.keyCode === 13) {
        term.writeln('');
        const cmd = commandBufferRef.current.trim();
        commandBufferRef.current = '';
        if (cmd) {
          executeCommand(cmd);
        } else {
          term.write('$ ');
        }
      } else if (domEvent.keyCode === 8) {
        if (commandBufferRef.current.length > 0) {
          commandBufferRef.current = commandBufferRef.current.slice(0, -1);
          term.write('\b \b');
        }
      } else if (printable) {
        commandBufferRef.current += key;
        term.write(key);
      }
    });

    const handleResize = () => {
      if (fitAddonRef.current) {
        fitAddonRef.current.fit();
      }
    };

    window.addEventListener('resize', handleResize);

    return () => {
      window.removeEventListener('resize', handleResize);
      unlisten.then((f) => f()).catch(() => {});
      invoke('close_adb_shell', { instanceId }).catch(() => {});
      term.dispose();
    };
  }, [instanceId]);

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-center gap-2 px-3 py-2 border-b bg-muted/50">
        <div className={`w-2 h-2 rounded-full ${connected ? 'bg-green-500' : 'bg-red-500'}`} />
        <span className="text-xs text-muted-foreground">
          {instanceId} • {connected ? 'Connected' : 'Disconnected'}
        </span>
      </div>
      <div 
        ref={terminalRef} 
        className="flex-1 bg-[#0a0a0a]"
        style={{ minHeight: '300px' }}
      />
    </div>
  );
}
