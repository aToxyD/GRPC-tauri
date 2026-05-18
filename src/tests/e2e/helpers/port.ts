import net from 'net';

/**
 * Checks if a port is currently active and accepting connections on 127.0.0.1.
 */
export function isPortInUse(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = new net.Socket();
    
    socket.once('connect', () => {
      socket.destroy();
      resolve(true); // Port is active and accepting connections
    });
    
    socket.once('error', () => {
      socket.destroy();
      resolve(false); // Port is closed
    });
    
    socket.connect(port, '127.0.0.1');
  });
}

/**
 * Waits until a port becomes active and accepts connections.
 */
export async function waitForPort(port: number, timeoutMs = 15000): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const inUse = await isPortInUse(port);
    if (inUse) {
      return; // Port is now active
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error(`Timeout waiting for port ${port} to be in use`);
}
