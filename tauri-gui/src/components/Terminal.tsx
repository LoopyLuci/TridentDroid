import { useEffect, useRef } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';

interface TerminalProps {
  instanceId: string;
  onCommand?: (cmd: string) => void;
}

export default function Terminal({ instanceId, onCommand }: TerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);

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

    let commandBuffer = '';

    term.writeln('\x1b[1;34mTridentDroid ADB Shell\x1b[0m');
    term.writeln(`\x1b[90mInstance: ${instanceId}\x1b[0m`);
    term.writeln('\x1b[90mConnecting...\x1b[0m');
    term.writeln('');

    term.onKey(({ key, domEvent }) => {
      const printable = !domEvent.altKey && !domEvent.ctrlKey && !domEvent.metaKey;
      
      if (domEvent.keyCode === 13) {
        // Enter
        term.writeln('');
        if (commandBuffer.trim() && onCommand) {
          onCommand(commandBuffer);
        }
        commandBuffer = '';
        term.write('$ ');
      } else if (domEvent.keyCode === 8) {
        // Backspace
        if (commandBuffer.length > 0) {
          commandBuffer = commandBuffer.slice(0, -1);
          term.write('\b \b');
        }
      } else if (printable) {
        commandBuffer += key;
        term.write(key);
      }
    });

    term.write('$ ');

    const handleResize = () => {
      if (fitAddonRef.current) {
        fitAddonRef.current.fit();
      }
    };

    window.addEventListener('resize', handleResize);

    return () => {
      window.removeEventListener('resize', handleResize);
      term.dispose();
    };
  }, [instanceId]);

  return (
    <div 
      ref={terminalRef} 
      className="h-full w-full bg-[#0a0a0a] rounded-lg overflow-hidden"
      style={{ minHeight: '300px' }}
    />
  );
}
