import crypto from "node:crypto";

export const TEST_SECRET = Buffer.from("my-ultra-secure-and-long-test-secret-32-bytes");
export const TEST_KID = "a1b2c3d4e5f60718293a4b5c6d7e8f90";

function base64Url(input: Buffer | string): string {
  const buf = typeof input === "string" ? Buffer.from(input, "utf-8") : input;
  return buf.toString("base64url");
}

export interface TestJwtClaims {
  sub?: string;
  role?: "client" | "admin" | "internal";
  jti?: string;
  exp?: number;
  nbf?: number;
  iat?: number;
  iss?: string;
  aud?: string;
}

export function createTestJwt(
  claimsOverride: Partial<TestJwtClaims> = {},
  kid: string = TEST_KID,
  secret: Buffer = TEST_SECRET
): string {
  const now = Math.floor(Date.now() / 1000);
  const header = {
    alg: "HS256",
    typ: "JWT",
    kid,
  };

  const payload: TestJwtClaims = {
    iss: "auth.internal",
    sub: crypto.randomUUID(),
    iat: now,
    nbf: now - 10,
    exp: now + 3600,
    aud: "order",
    role: "client",
    jti: crypto.randomUUID(),
    ...claimsOverride,
  };

  const encodedHeader = base64Url(JSON.stringify(header));
  const encodedPayload = base64Url(JSON.stringify(payload));
  const signatureInput = `${encodedHeader}.${encodedPayload}`;

  const hmac = crypto.createHmac("sha256", secret);
  hmac.update(signatureInput);
  const signature = base64Url(hmac.digest());

  return `${signatureInput}.${signature}`;
}

export function getTestJwks(kid: string = TEST_KID, secret: Buffer = TEST_SECRET) {
  return {
    keys: [
      {
        kty: "oct",
        kid,
        alg: "HS256",
        k: base64Url(secret),
      },
    ],
  };
}
