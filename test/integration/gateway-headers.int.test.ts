import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startMockUpstream, type MockUpstreamServer } from "../fixtures/mock-upstream.js";
import { createTestJwt } from "../fixtures/jwt.js";
import { startGateway, type GatewayProcess } from "../helpers/gateway-process.js";

describe("Gateway Header Sanitization", () => {
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

  it("strips spoofed user ID and custom headers when client sends them", async () => {
    const userId = "4b684cb3-0098-4c6e-884e-d67b2d28f091";
    const tokenId = "7c8234bb-6681-4f9e-a035-7cfa85fba331";

    const validToken = createTestJwt({
      sub: userId,
      jti: tokenId,
      role: "client",
    });

    const body = JSON.stringify({
      symbol: "BTC_USDT",
      side: "buy",
      order_type: "limit",
      price: "68000.00",
      amount: "1.00",
    });

    const res = await fetch(`${gateway.url}/api/v1/orders/create`, {
      method: "POST",
      headers: {
        authorization: `Bearer ${validToken}`,
        "content-type": "application/json",
        accept: "application/json",
        "x-user-id": "attacker-spoofed-user-id",
        "x-user-role": "admin",
        "x-token-id": "attacker-spoofed-token-id",
        "x-forwarded-for": "198.51.100.1",
        "x-forwarded-proto": "https",
        "x-real-ip": "198.51.100.1",
        "x-custom-misc": "random-arbitrary-data",
        cookie: "session_id=fake_session",
        "if-none-match": 'W/"fake-etag"',
        "cache-control": "no-cache",
        "sec-ch-ua": '"Chromium";v="120"',
        "sec-fetch-dest": "empty",
      },
      body,
    });

    expect(res.status).toBe(200);

    const upstream = mockServer.getLastReceivedRequest();
    expect(upstream).not.toBeNull();
    if (!upstream) return;

    expect(upstream.headers["x-user-id"]).toBe(userId);
    expect(upstream.headers["x-user-role"]).toBe("client");
    expect(upstream.headers["x-token-id"]).toBe(tokenId);

    expect(upstream.headers["authorization"]).toBeUndefined();
    expect(upstream.headers["x-forwarded-for"]).toBeUndefined();
    expect(upstream.headers["x-forwarded-proto"]).toBeUndefined();
    expect(upstream.headers["x-real-ip"]).toBeUndefined();
    expect(upstream.headers["x-custom-misc"]).toBeUndefined();
    expect(upstream.headers["cookie"]).toBeUndefined();
    expect(upstream.headers["if-none-match"]).toBeUndefined();
    expect(upstream.headers["cache-control"]).toBeUndefined();
    expect(upstream.headers["sec-ch-ua"]).toBeUndefined();
    expect(upstream.headers["sec-fetch-dest"]).toBeUndefined();

    expect(upstream.headers["content-type"]).toBe("application/json");
    expect(upstream.headers["accept"]).toBe("application/json");

    expect(upstream.body).toBe(body);
  });

  it("mints a fresh UUIDv7 request ID when request contains client-provided ID", async () => {
    const validToken = createTestJwt();
    const spoofedRequestId = "00000000-0000-0000-0000-000000000000";

    const res = await fetch(`${gateway.url}/api/v1/orders/open`, {
      method: "GET",
      headers: {
        authorization: `Bearer ${validToken}`,
        "x-request-id": spoofedRequestId,
      },
    });

    expect(res.status).toBe(200);

    const clientReturnedRequestId = res.headers.get("x-request-id");
    expect(clientReturnedRequestId).toBeTruthy();
    expect(clientReturnedRequestId).not.toBe(spoofedRequestId);

    const uuidv7Regex = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
    expect(clientReturnedRequestId).toMatch(uuidv7Regex);

    const upstream = mockServer.getLastReceivedRequest();
    expect(upstream).not.toBeNull();
    if (!upstream) return;

    expect(upstream.headers["x-request-id"]).toBe(clientReturnedRequestId);
  });

  it("injects authenticated user ID and role when valid JWT is presented", async () => {
    const partnerId = "11111111-2222-3333-4444-555555555555";
    const partnerToken = createTestJwt({
      sub: partnerId,
      role: "partner",
    });

    const res = await fetch(`${gateway.url}/api/v1/ledger/balances`, {
      method: "GET",
      headers: {
        authorization: `Bearer ${partnerToken}`,
      },
    });

    expect(res.status).toBe(200);

    const upstream = mockServer.getLastReceivedRequest();
    expect(upstream).not.toBeNull();
    if (!upstream) return;

    expect(upstream.headers["x-user-id"]).toBe(partnerId);
    expect(upstream.headers["x-user-role"]).toBe("partner");
  });
});
