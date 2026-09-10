// SPIKE ONLY. The smallest possible plugin: take the port, say ready.
// Used only to time how much a note pays per block for the frame itself.
window.addEventListener('message', (e) => {
  const port = e.ports[0];
  if (port) port.postMessage({ v: 1, method: 'report', params: { ready: true } });
});
