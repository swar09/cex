import { describe, it, expect, beforeAll, afterAll } from "vitest";
import http from "node:http";
import { startMockUpstream, type MockUpstreamServer } from "../fixtures/mock-upstream.js";
import { createTestJwt, TEST_KID } from "../fixtures/jwt.js";
import { startGateway, type GatewayProcess } from "../helpers/gateway-process.js";

describe("gateway e2e", () => {
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

  describe("tracing", () => {
    it("returns 200 and mints uuidv7 request id", async () => {
      const res = await fetch(`${gateway.url}/healthz`);
      expect(res.status).toBe(200);
      expect(await res.text()).toBe("OK");

      const requestId = res.headers.get("x-request-id");
      expect(requestId).toBeTruthy();

      const uuidRegex = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
      expect(requestId).toMatch(uuidRegex);
    });

    it("overwrites incoming client request id with minted uuidv7", async () => {
      const spoofedId = "11111111-1111-1111-1111-111111111111";
      const res = await fetch(`${gateway.url}/healthz`, {
        headers: {
          "x-request-id": spoofedId,
        },
      });

      expect(res.status).toBe(200);
      const requestId = res.headers.get("x-request-id");
      expect(requestId).not.toBe(spoofedId);
      expect(requestId).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-7/i);
    });
  });

  describe("auth", () => {
    it("rejects unauthenticated requests to protected endpoints", async () => {
      const res = await fetch(`${gateway.url}/api/v1/orders/create`, {
        method: "POST",
        body: JSON.stringify({ symbol: "BTC/USDT" }),
      });

      expect(res.status).toBe(401);
      expect(res.headers.get("x-request-id")).toBeTruthy();
    });

    it("rejects malformed token strings", async () => {
      const res = await fetch(`${gateway.url}/api/v1/orders/create`, {
        method: "POST",
        headers: {
          authorization: "Bearer not-a-valid-jwt-token",
        },
      });

      expect(res.status).toBe(401);
    });

    it("rejects token with non-32-char hex kid format", async () => {
      const invalidKidToken = createTestJwt({}, "invalid-short-kid");
      const res = await fetch(`${gateway.url}/api/v1/orders/create`, {
        method: "POST",
        headers: {
          authorization: `Bearer ${invalidKidToken}`,
        },
      });

      expect(res.status).toBe(401);
    });

    it("rejects expired token", async () => {
      const expiredToken = createTestJwt({
        exp: Math.floor(Date.now() / 1000) - 3600,
      });

      const res = await fetch(`${gateway.url}/api/v1/orders/create`, {
        method: "POST",
        headers: {
          authorization: `Bearer ${expiredToken}`,
        },
      });

      expect(res.status).toBe(401);
    });
  });

  describe("sanitization", () => {
    it("purges spoofed headers and injects authentic token claims", async () => {
      const userId = "4b684cb3-0098-4c6e-884e-d67b2d28f091";
      const tokenId = "7c8234bb-6681-4f9e-a035-7cfa85fba331";

      const validToken = createTestJwt({
        sub: userId,
        jti: tokenId,
        role: "client",
      });

      const orderPayload = JSON.stringify({
        symbol: "BTC/USDT",
        amount: 1.5,
        price: 68500,
      });

      const res = await fetch(`${gateway.url}/api/v1/orders/create?mode=immediate`, {
        method: "POST",
        headers: {
          authorization: `Bearer ${validToken}`,
          "content-type": "application/json",
          "x-user-id": "attacker-spoofed-user-id",
          "x-user-role": "SuperAdmin",
          "x-token-id": "attacker-spoofed-token-id",
          "x-forwarded-for": "203.0.113.195",
          "x-custom-header": "malicious-injection",
          cookie: "session=stolen_session_id",
          "if-none-match": 'W/"etag-spoof-123"',
          "cache-control": "no-cache",
        },
        body: orderPayload,
      });

      expect(res.status).toBe(200);

      const upstream = mockServer.getLastReceivedRequest();
      expect(upstream).not.toBeNull();
      if (!upstream) return;

      expect(upstream.url).toBe("/api/v1/orders/create?mode=immediate");

      expect(upstream.headers["x-user-id"]).toBe(userId);
      expect(upstream.headers["x-user-role"]).toBe("client");
      expect(upstream.headers["x-token-id"]).toBe(tokenId);

      expect(upstream.headers["authorization"]).toBeUndefined();
      expect(upstream.headers["x-forwarded-for"]).toBeUndefined();
      expect(upstream.headers["x-custom-header"]).toBeUndefined();
      expect(upstream.headers["cookie"]).toBeUndefined();
      expect(upstream.headers["if-none-match"]).toBeUndefined();
      expect(upstream.headers["cache-control"]).toBeUndefined();

      expect(upstream.headers["content-type"]).toBe("application/json");

      expect(upstream.body).toBe(orderPayload);
    });
  });

  describe("proxy limits", () => {
    it("returns 413 when payload exceeds 10mb limit", async () => {
      const statusCode = await new Promise<number>((resolve, reject) => {
        const req = http.request(
          `${gateway.url}/healthz`,
          {
            method: "POST",
            headers: {
              "content-length": "20000000",
            },
          },
          (res) => resolve(res.statusCode || 0)
        );
        req.on("error", reject);
        req.end();
      });

      expect(statusCode).toBe(413);
    });

    it("forwards streaming payloads transparently", async () => {
      const validToken = createTestJwt();
      const largePayload = JSON.stringify({
        items: Array.from({ length: 500 }, (_, i) => ({
          id: i,
          price: (i * 1.25).toFixed(2),
          timestamp: Date.now(),
        })),
      });

      const res = await fetch(`${gateway.url}/api/v1/orders/batch`, {
        method: "POST",
        headers: {
          authorization: `Bearer ${validToken}`,
          "content-type": "application/json",
        },
        body: largePayload,
      });

      expect(res.status).toBe(200);
      const upstream = mockServer.getLastReceivedRequest();
      expect(upstream?.body).toBe(largePayload);
    });
  });
});
