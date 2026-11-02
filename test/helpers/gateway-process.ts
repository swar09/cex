import { spawn, type ChildProcess } from "node:child_process";
import net from "node:net";
import path from "node:path";
import fs from "node:fs";

export interface GatewayProcess {
  url: string;
  port: number;
  stop: () => Promise<void>;
}

function getFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, "127.0.0.1", () => {
      const addr = srv.address();
      if (!addr || typeof addr === "string") {
        reject(new Error("Unable to obtain free port"));
        return;
      }
      const port = addr.port;
      srv.close((err) => (err ? reject(err) : resolve(port)));
    });
  });
}

export async function startGateway(mockUpstreamUrl: string): Promise<GatewayProcess> {
  const binaryPath = "/tmp/cargo-target/debug/api-gateway";
  if (!fs.existsSync(binaryPath)) {
    throw new Error(`api-gateway binary not found at ${binaryPath}. Run 'cargo build -p api-gateway' first.`);
  }

  const port = await getFreePort();
  const url = `http://127.0.0.1:${port}`;

  const env = {
    ...process.env,
    SERVER_HOST: "127.0.0.1",
    SERVER_PORT: port.toString(),
    GLOBAL_JWKS_ENDPOINT: `${mockUpstreamUrl}/.well_known/jwks.json`,
    ORDER_SERVICE_URL: mockUpstreamUrl,
    ENGINE_SERVICE_URL: mockUpstreamUrl,
    LEDGER_SERVICE_URL: mockUpstreamUrl,
    USER_SERVICE_URL: mockUpstreamUrl,
    KYC_SERVICE_URL: mockUpstreamUrl,
    AUTH_SERVICE_URL: mockUpstreamUrl,
  };

  const child: ChildProcess = spawn(binaryPath, [], {
    env,
    stdio: ["ignore", "pipe", "pipe"],
  });

  child.stderr?.on("data", (data) => {
    const msg = data.toString();
    if (!msg.includes("redis connection unavailable")) {
      // Suppress normal in-memory fallback log; surface unexpected errors
    }
  });

  // Poll /healthz until 200 OK
  const deadline = Date.now() + 10000;
  let ready = false;

  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${url}/healthz`);
      if (res.status === 200) {
        ready = true;
        break;
      }
    } catch {
      // Gateway still booting
    }
    await new Promise((r) => setTimeout(r, 25));
  }

  if (!ready) {
    child.kill("SIGKILL");
    throw new Error(`Gateway failed to become ready at ${url}/healthz within 10s`);
  }

  return {
    url,
    port,
    stop: () =>
      new Promise<void>((resolve) => {
        if (child.killed || child.exitCode !== null) {
          resolve();
          return;
        }
        child.once("exit", () => resolve());
        child.kill("SIGTERM");
        // Force kill if not exited after 1s
        setTimeout(() => {
          if (child.exitCode === null) {
            child.kill("SIGKILL");
          }
        }, 1000);
      }),
  };
}
