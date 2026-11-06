import { describe, it, expect, beforeAll, afterAll } from "vitest";
import http from "node:http";
import { startMockUpstream, type MockUpstreamServer } from "../fixtures/mock-upstream.js";
import { createTestJwt } from "../fixtures/jwt.js";
import { startGateway, type GatewayProcess } from "../helpers/gateway-process.js";

describe("Gateway Access Control and Routing", () => {
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

  describe("Public Routes", () => {
    it("returns 200 OK when requesting health check without token", async () => {
      const res = await fetch(`${gateway.url}/healthz`);
      expect(res.status).toBe(200);
      expect(await res.text()).toBe("OK");
    });

    it("proxies to auth service when logging in without existing token", async () => {
      const loginPayload = JSON.stringify({
        email: "trader@example.com",
        password: "SecretPassword123!",
      });

      const res = await fetch(`${gateway.url}/api/v1/auth/login`, {
        method: "POST",
        headers: {
          "content-type": "application/json",
        },
        body: loginPayload,
      });

      expect(res.status).toBe(200);
      const upstream = mockServer.getLastReceivedRequest();
      expect(upstream).not.toBeNull();
      if (!upstream) return;

      expect(upstream.url).toBe("/api/v1/auth/login");
      expect(upstream.body).toBe(loginPayload);
    });

    it("proxies to auth service when registering new user without token", async () => {
      const registerPayload = JSON.stringify({
        email: "newtrader@example.com",
        password: "SecretPassword123!",
      });

      const res = await fetch(`${gateway.url}/api/v1/auth/signup`, {
        method: "POST",
        headers: {
          "content-type": "application/json",
        },
        body: registerPayload,
      });

      expect(res.status).toBe(200);
      const upstream = mockServer.getLastReceivedRequest();
      expect(upstream).not.toBeNull();
      if (!upstream) return;

      expect(upstream.url).toBe("/api/v1/auth/signup");
    });
  });

  describe("Protected Routes", () => {
    const protectedPaths = [
      "/api/v1/orders/open",
      "/api/v1/engine/status",
      "/api/v1/ledger/balances",
      "/api/v1/user/me",
      "/api/v1/kyc/status",
    ];

    for (const path of protectedPaths) {
      it(`returns 401 Unauthorized when accessing ${path} without token`, async () => {
        const res = await fetch(`${gateway.url}${path}`);
        expect(res.status).toBe(401);
      });
    }

    it("returns 401 Unauthorized when token is missing Bearer prefix", async () => {
      const token = createTestJwt();
      const res = await fetch(`${gateway.url}/api/v1/orders/open`, {
        headers: {
          authorization: token,
        },
      });
      expect(res.status).toBe(401);
    });

    it("returns 401 Unauthorized when token kid is not 32-character hex", async () => {
      const token = createTestJwt({}, "invalid-short-kid");
      const res = await fetch(`${gateway.url}/api/v1/orders/open`, {
        headers: {
          authorization: `Bearer ${token}`,
        },
      });
      expect(res.status).toBe(401);
    });

    it("returns 401 Unauthorized when token expiration is in the past", async () => {
      const expiredToken = createTestJwt({
        exp: Math.floor(Date.now() / 1000) - 1000,
      });
      const res = await fetch(`${gateway.url}/api/v1/orders/open`, {
        headers: {
          authorization: `Bearer ${expiredToken}`,
        },
      });
      expect(res.status).toBe(401);
    });

    it("triggers only one JWKS fetch and activates cooldown when swarmed with invalid key IDs", async () => {
      mockServer.resetJwksRequestCount();

      const invalidKids = Array.from({ length: 25 }, (_, i) =>
        (i + 1).toString(16).padStart(32, "0")
      );

      const responses = await Promise.all(
        invalidKids.map((kid) =>
          fetch(`${gateway.url}/api/v1/orders/open`, {
            headers: {
              authorization: `Bearer ${createTestJwt({}, kid)}`,
            },
          })
        )
      );

      for (const res of responses) {
        expect(res.status).toBe(401);
      }

      expect(mockServer.getJwksRequestCount()).toBe(1);
    });
  });

  describe("Upstream Proxy", () => {
    it("preserves query parameters when forwarding request to downstream service", async () => {
      const validToken = createTestJwt();
      const res = await fetch(`${gateway.url}/api/v1/orders/open?symbol=BTC_USDT&limit=25`, {
        headers: {
          authorization: `Bearer ${validToken}`,
        },
      });

      expect(res.status).toBe(200);
      const upstream = mockServer.getLastReceivedRequest();
      expect(upstream).not.toBeNull();
      if (!upstream) return;

      expect(upstream.url).toBe("/api/v1/orders/open?symbol=BTC_USDT&limit=25");
    });

    it("returns 404 Not Found when route has no upstream service mapping", async () => {
      const validToken = createTestJwt();
      const res = await fetch(`${gateway.url}/api/v1/unknown_service/foo`, {
        headers: {
          authorization: `Bearer ${validToken}`,
        },
      });

      expect(res.status).toBe(404);
    });
  });

  describe("Traffic Limits", () => {
    it("returns 413 Payload Too Large when request body exceeds 10MB limit", async () => {
      const statusCode = await new Promise<number>((resolve, reject) => {
        const req = http.request(
          `${gateway.url}/healthz`,
          {
            method: "POST",
            headers: {
              "content-length": "15000000",
            },
          },
          (res) => resolve(res.statusCode || 0)
        );
        req.on("error", reject);
        req.end();
      });

      expect(statusCode).toBe(413);
    });

    it("streams request body without corruption when payload is within limits", async () => {
      const validToken = createTestJwt();
      const payloadObject = {
        orders: Array.from({ length: 20 }, (_, idx) => ({
          symbol: "BTC_USDT",
          side: idx % 2 === 0 ? "buy" : "sell",
          order_type: "limit",
          price: (60000 + idx * 10).toFixed(2),
          amount: (0.1 * (idx + 1)).toFixed(2),
        })),
      };

      const payload = JSON.stringify(payloadObject);

      const res = await fetch(`${gateway.url}/api/v1/orders/batch`, {
        method: "POST",
        headers: {
          authorization: `Bearer ${validToken}`,
          "content-type": "application/json",
        },
        body: payload,
      });

      expect(res.status).toBe(200);
      const upstream = mockServer.getLastReceivedRequest();
      expect(upstream).not.toBeNull();
      if (!upstream) return;

      expect(upstream.body).toBe(payload);
      expect(JSON.parse(upstream.body)).toEqual(payloadObject);
    });

    it("returns 504 Gateway Timeout when downstream service exceeds request timeout", async () => {
      const slowMockServer = await startMockUpstream();
      slowMockServer.setResponseDelayMs(1500);

      const timedGateway = await startGateway(slowMockServer.url, {
        REQUEST_TIMEOUT_SECS: "1",
      });

      try {
        const validToken = createTestJwt();
        const res = await fetch(`${timedGateway.url}/api/v1/orders/open`, {
          headers: {
            authorization: `Bearer ${validToken}`,
          },
        });

        expect(res.status).toBe(504);
      } finally {
        await timedGateway.stop();
        await slowMockServer.close();
      }
    }, 15000);
  });
});
