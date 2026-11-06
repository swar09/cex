import http from "node:http";
import { getTestJwks } from "./jwt.js";

export interface MockUpstreamServer {
  url: string;
  port: number;
  close: () => Promise<void>;
  getLastReceivedRequest: () => EchoPayload | null;
  getJwksRequestCount: () => number;
  resetJwksRequestCount: () => void;
  setResponseDelayMs: (ms: number) => void;
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
    let jwksCount = 0;
    let responseDelayMs = 0;

    const server = http.createServer((req, res) => {
      if (req.url === "/.well_known/jwks.json" && req.method === "GET") {
        jwksCount++;
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

        const execute = () => {
          res.writeHead(200, { "content-type": "application/json" });
          res.end(JSON.stringify(lastRequest));
        };

        if (responseDelayMs > 0) {
          setTimeout(execute, responseDelayMs);
        } else {
          execute();
        }
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
        getJwksRequestCount: () => jwksCount,
        resetJwksRequestCount: () => {
          jwksCount = 0;
        },
        setResponseDelayMs: (ms: number) => {
          responseDelayMs = ms;
        },
        close: () =>
          new Promise((done, err) => {
            server.close((e) => (e ? err(e) : done()));
          }),
      });
    });
  });
}
