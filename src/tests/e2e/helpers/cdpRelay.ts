/**
 * cdpRelay.ts
 *
 * A low-level, high-performance TCP proxy that transparently bridges
 * Playwright's npm `ws` package to WebView2's Chrome DevTools Protocol port.
 */

import net from 'net';

export interface CdpRelayHandle {
  stop(): void;
}

export function startCdpRelay(realPort: number, relayPort: number): CdpRelayHandle {
  const server = net.createServer((clientSocket) => {
    clientSocket.setNoDelay(true);
    console.log('[cdpRelay] New TCP client connection accepted');
    
    const upstream = net.createConnection({ port: realPort, host: '127.0.0.1' });
    upstream.setNoDelay(true);

    let upstreamConnected = false;
    const clientQueue: Buffer[] = [];

    upstream.on('connect', () => {
      console.log(`[cdpRelay] Connected to upstream WebView2 on port ${realPort}`);
      upstreamConnected = true;
      
      // Flush queued client data
      for (const rawChunk of clientQueue) {
        const chunk = Buffer.isBuffer(rawChunk) ? rawChunk : Buffer.from(rawChunk as any);
        const str = chunk.toString('utf8');
        const target1 = `127.0.0.1:${relayPort}`;
        const target2 = `localhost:${relayPort}`;

        if (str.includes(target1) || str.includes(target2)) {
          console.log(`[cdpRelay] Flush-rewriting client request: Port ${relayPort} -> ${realPort}`);
          const rewritten = str
            .replaceAll(target1, `127.0.0.1:${realPort}`)
            .replaceAll(target2, `127.0.0.1:${realPort}`);
          upstream.write(Buffer.from(rewritten, 'utf8'));
        } else {
          console.log(`[cdpRelay] Flush-forwarding raw client request (${chunk.length} bytes)`);
          upstream.write(chunk);
        }
      }
      clientQueue.length = 0;
    });

    // ── 1. Forward and rewrite client -> upstream (WebView2) ──────────────────
    clientSocket.on('data', (rawChunk) => {
      const chunk = Buffer.isBuffer(rawChunk) ? rawChunk : Buffer.from(rawChunk as any);
      
      if (!upstreamConnected) {
        console.log(`[cdpRelay] Buffering client chunk (${chunk.length} bytes) before upstream is connected`);
        clientQueue.push(chunk);
        return;
      }

      const str = chunk.toString('utf8');
      const target1 = `127.0.0.1:${relayPort}`;
      const target2 = `localhost:${relayPort}`;

      if (str.includes(target1) || str.includes(target2)) {
        console.log(`[cdpRelay] Rewriting client request: Port ${relayPort} -> ${realPort}`);
        const rewritten = str
          .replaceAll(target1, `127.0.0.1:${realPort}`)
          .replaceAll(target2, `127.0.0.1:${realPort}`);
        upstream.write(Buffer.from(rewritten, 'utf8'));
      } else {
        console.log(`[cdpRelay] Forwarding raw client data (${chunk.length} bytes)`);
        upstream.write(chunk);
      }
    });

    // ── 2. Forward and rewrite upstream (WebView2) -> client ──────────────────
    upstream.on('data', (rawChunk) => {
      const chunk = Buffer.isBuffer(rawChunk) ? rawChunk : Buffer.from(rawChunk as any);
      const str = chunk.toString('utf8');
      const target1 = `127.0.0.1:${realPort}`;
      const target2 = `localhost:${realPort}`;

      if (str.includes(target1) || str.includes(target2)) {
        console.log(`[cdpRelay] Rewriting upstream response: Port ${realPort} -> ${relayPort}`);
        const rewritten = str
          .replaceAll(target1, `127.0.0.1:${relayPort}`)
          .replaceAll(target2, `127.0.0.1:${relayPort}`);

        if (str.startsWith('HTTP/') && !str.includes('101 ')) {
          const finalized = rewritten
            .replace(/Connection:\s*keep-alive/gi, 'Connection: close')
            .replace(/connection:\s*keep-alive/gi, 'Connection: close');
          
          clientSocket.write(Buffer.from(finalized, 'utf8'), () => {
            console.log('[cdpRelay] Gracefully tearing down keep-alive HTTP connection');
            try { clientSocket.end(); } catch {}
            try { upstream.end(); } catch {}
          });
        } else {
          clientSocket.write(Buffer.from(rewritten, 'utf8'));
        }
      } else {
        console.log(`[cdpRelay] Forwarding raw upstream data (${chunk.length} bytes) to client`);
        clientSocket.write(chunk);
      }
    });

    // ── 3. Wire up connection teardown ────────────────────────────────────────
    upstream.on('error', (err) => {
      console.error('[cdpRelay] Upstream socket error:', err);
      try { clientSocket.destroy(); } catch {}
    });
    clientSocket.on('error', (err) => {
      console.error('[cdpRelay] Client socket error:', err);
      try { upstream.destroy(); } catch {}
    });
    upstream.on('close', () => {
      console.log('[cdpRelay] Upstream socket closed');
      try { clientSocket.destroy(); } catch {}
    });
    clientSocket.on('close', () => {
      console.log('[cdpRelay] Client socket closed');
      try { upstream.destroy(); } catch {}
    });
  });

  server.listen(relayPort, '127.0.0.1');

  return {
    stop() {
      server.close();
    },
  };
}
