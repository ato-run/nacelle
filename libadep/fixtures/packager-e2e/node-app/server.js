import http from 'node:http';

const port = Number(process.env.PORT ?? '3000');

const server = http.createServer((req, res) => {
  res.writeHead(200, { 'content-type': 'text/plain; charset=utf-8' });
  res.end('ok: node\n');
});

server.listen(port, '0.0.0.0', () => {
  console.log(`listening on ${port}`);
});
