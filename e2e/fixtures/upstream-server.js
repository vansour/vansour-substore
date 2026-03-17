const http = require("http");

const port = Number(process.env.UPSTREAM_PORT || 4181);
const host = "127.0.0.1";

const server = http.createServer((req, res) => {
  if (req.url === "/healthz") {
    res.writeHead(200, { "content-type": "text/plain; charset=utf-8" });
    res.end("ok");
    return;
  }

  if (req.url === "/feed") {
    res.writeHead(200, { "content-type": "text/plain; charset=utf-8" });
    res.end("fixture-line-1\nfixture-line-2\nfixture-line-3\n");
    return;
  }

  res.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
  res.end("not found");
});

function shutdown() {
  server.close(() => process.exit(0));
}

server.listen(port, host, () => {
  process.stdout.write(`upstream fixture listening on http://${host}:${port}\n`);
});

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
