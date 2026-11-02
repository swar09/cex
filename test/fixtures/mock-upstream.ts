import http from "node:http";
import { getTestJwks } from "./jwt.js";

export interface MockUpstreamServer {
  url: string;
  port: number;
  close: () => Promise<void>;
  getLastReceivedRequest: () => EchoPayload | null;
}

export interface EchoPayload {
  method: string;
  url: string;
  headers: Record<string, string | string[] | undefined>;
  body: string;
}

export function startMockUpstream(): Promise<MockUpstreamServer> {
  return new Promise((resolve, reject) => {
    let lastRequest: EchoPayload | null = null;

    const server = http.createServer((req, res) => {
      if (req.url === "/.well_known/jwks.json" && req.method === "GET") {
        const jwks = getTestJwks();
        res.writeHead(200, { "content-type": "application/json" });
        res.end(JSON.stringify(jwks));
        return;
      }

      const chunks: Buffer[] = [];
      req.on("data", (chunk) => chunks.push(chunk));
      req.on("end", () => {
        const body = Buffer.concat(chunks).toString("utf-8");
        lastRequest = {
          method: req.method || "GET",
          url: req.url || "/",
          headers: req.headers,
          body,
        };

        res.writeHead(200, { "content-type": "application/json" });
        res.end(JSON.stringify(lastRequest));
      });
    });

    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        reject(new Error("Failed to get server port"));
        return;
      }

      const port = address.port;
      const url = `http://127.0.0.1:${port}`;

      resolve({
        url,
        port,
        getLastReceivedRequest: () => lastRequest,
        close: () =>
          new Promise((done, err) => {
            server.close((e) => (e ? err(e) : done()));
          }),
      });
    });
  });
}
