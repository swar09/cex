import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startMockUpstream, type MockUpstreamServer } from "../fixtures/mock-upstream.js";
import { startGateway, type GatewayProcess } from "../helpers/gateway-process.js";

describe("Gateway Cross-Origin Resource Sharing (CORS)", () => {
  let mockServer: MockUpstreamServer;
  let gateway: GatewayProcess;

  beforeAll(async () => {
    mockServer = await startMockUpstream();
    gateway = await startGateway(mockServer.url);
  }, 20000);

  afterAll(async () => {
    await gateway?.stop();
    await mockServer?.close();
  });

  it("returns access control allow headers when options preflight request is sent", async () => {
    const origin = "https://app.cex.io";
    const res = await fetch(`${gateway.url}/api/v1/orders/open`, {
      method: "OPTIONS",
      headers: {
        Origin: origin,
        "Access-Control-Request-Method": "POST",
        "Access-Control-Request-Headers": "authorization,content-type",
      },
    });

    expect(res.status).toBe(200);
    expect(res.headers.get("access-control-allow-origin")).toBe(origin);
    expect(res.headers.get("access-control-allow-credentials")).toBe("true");

    const allowMethods = res.headers.get("access-control-allow-methods") || "";
    expect(allowMethods).toContain("POST");
    expect(allowMethods).toContain("GET");
    expect(allowMethods).toContain("OPTIONS");

    const allowHeaders = res.headers.get("access-control-allow-headers") || "";
    expect(allowHeaders.toLowerCase()).toContain("authorization");
    expect(allowHeaders.toLowerCase()).toContain("content-type");
  });

  it("mirrors request origin in allow origin header when simple cross origin request is received", async () => {
    const origin = "https://dashboard.cex.io";
    const res = await fetch(`${gateway.url}/healthz`, {
      method: "GET",
      headers: {
        Origin: origin,
      },
    });

    expect(res.status).toBe(200);
    expect(res.headers.get("access-control-allow-origin")).toBe(origin);
    expect(res.headers.get("access-control-allow-credentials")).toBe("true");
  });
});
