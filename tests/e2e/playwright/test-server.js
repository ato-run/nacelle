/**
 * Simple HTTP server for Playwright testing
 * Serves the coordinator UI without requiring full coordinator dependencies
 */
const http = require('http');
const fs = require('fs');
const path = require('path');

const PORT = process.env.PORT || 8080;

// Path to the web UI
const webDir = path.join(__dirname, '../../../client/pkg/httpserver/web');
const indexPath = path.join(webDir, 'index.html');

// Simple health response
const healthResponse = {
  status: 'healthy',
  uptime: '0s',
  timestamp: new Date().toISOString(),
  version: '1.0.0-test'
};

const server = http.createServer((req, res) => {
  console.log(`${req.method} ${req.url}`);

  // Health endpoints
  if (req.url === '/health') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(healthResponse));
    return;
  }

  if (req.url === '/ready') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ status: 'ready', ...healthResponse }));
    return;
  }

  if (req.url === '/live') {
    res.writeHead(200, { 'Content-Type': 'text/plain' });
    res.end('alive');
    return;
  }

  // Serve index.html for root
  if (req.url === '/' || req.url === '/index.html') {
    fs.readFile(indexPath, 'utf8', (err, content) => {
      if (err) {
        res.writeHead(500, { 'Content-Type': 'text/plain' });
        res.end('Error loading index.html: ' + err.message);
        return;
      }
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(content);
    });
    return;
  }

  // 404 for everything else
  res.writeHead(404, { 'Content-Type': 'text/plain' });
  res.end('Not Found');
});

server.listen(PORT, () => {
  console.log(`Test server running at http://localhost:${PORT}/`);
  console.log(`Serving UI from: ${webDir}`);
  console.log('Press Ctrl+C to stop');
});

// Graceful shutdown
process.on('SIGTERM', () => {
  console.log('Received SIGTERM, shutting down...');
  server.close(() => {
    console.log('Server closed');
    process.exit(0);
  });
});

process.on('SIGINT', () => {
  console.log('Received SIGINT, shutting down...');
  server.close(() => {
    console.log('Server closed');
    process.exit(0);
  });
});
