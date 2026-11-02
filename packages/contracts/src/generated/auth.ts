import { makeApi, Zodios, type ZodiosOptions } from "@zodios/core";
import { z } from "zod";

const SignupRequest = z
  .object({
    email: z.string().email(),
    password: z.string().min(8).max(128),
    referral_code: z.string().optional(),
  })
  .passthrough();
const AuthResponse = z
  .object({
    access_token: z.string(),
    token_type: z.string(),
    expires_in: z.number().int(),
    user_id: z.string().uuid(),
  })
  .passthrough();
const ErrorResponse = z
  .object({ code: z.string(), message: z.string() })
  .passthrough();
const LoginRequest = z
  .object({ email: z.string().email(), password: z.string() })
  .passthrough();
const SuccessResponse = z.object({ success: z.boolean() }).passthrough();
const UpdatePasswordRequest = z
  .object({
    current_password: z.string(),
    new_password: z.string().min(8).max(128),
  })
  .passthrough();
const VerifyPasswordRequest = z.object({ password: z.string() }).passthrough();
const JwksResponse = z
  .object({
    keys: z.array(
      z
        .object({
          kty: z.string(),
          kid: z.string().regex(/^[0-9a-f]{32}$/),
          alg: z.string(),
          k: z.string().optional(),
        })
        .passthrough()
    ),
  })
  .passthrough();

export const schemas = {
  SignupRequest,
  AuthResponse,
  ErrorResponse,
  LoginRequest,
  SuccessResponse,
  UpdatePasswordRequest,
  VerifyPasswordRequest,
  JwksResponse,
};

const endpoints = makeApi([
  {
    method: "get",
    path: "/.well_known/jwks.json",
    alias: "getJwks",
    requestFormat: "json",
    response: JwksResponse,
  },
  {
    method: "post",
    path: "/api/v1/auth/login",
    alias: "login",
    requestFormat: "json",
    parameters: [
      {
        name: "body",
        type: "Body",
        schema: LoginRequest,
      },
    ],
    response: AuthResponse,
    errors: [
      {
        status: 401,
        description: `Invalid credentials`,
        schema: ErrorResponse,
      },
    ],
  },
  {
    method: "post",
    path: "/api/v1/auth/logout",
    alias: "logout",
    requestFormat: "json",
    parameters: [
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
      {
        name: "x-token-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: z.object({ success: z.boolean() }).passthrough(),
    errors: [
      {
        status: 401,
        description: `Unauthorized`,
        schema: z.void(),
      },
    ],
  },
  {
    method: "get",
    path: "/api/v1/auth/oauth/google",
    alias: "oauthGoogle",
    requestFormat: "json",
    response: z.void(),
    errors: [
      {
        status: 302,
        description: `Redirect to Google consent screen`,
        schema: z.void(),
      },
    ],
  },
  {
    method: "get",
    path: "/api/v1/auth/oauth/google/callback",
    alias: "oauthGoogleCallback",
    requestFormat: "json",
    parameters: [
      {
        name: "code",
        type: "Query",
        schema: z.string(),
      },
      {
        name: "state",
        type: "Query",
        schema: z.string(),
      },
    ],
    response: AuthResponse,
    errors: [
      {
        status: 400,
        description: `Invalid state or exchange failure`,
        schema: ErrorResponse,
      },
    ],
  },
  {
    method: "post",
    path: "/api/v1/auth/password/update",
    alias: "updatePassword",
    requestFormat: "json",
    parameters: [
      {
        name: "body",
        type: "Body",
        schema: UpdatePasswordRequest,
      },
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: z.object({ success: z.boolean() }).passthrough(),
    errors: [
      {
        status: 400,
        description: `Weak password or invalid parameters`,
        schema: ErrorResponse,
      },
      {
        status: 401,
        description: `Invalid current password or unauthorized`,
        schema: z.void(),
      },
    ],
  },
  {
    method: "post",
    path: "/api/v1/auth/password/verify",
    alias: "verifyPassword",
    requestFormat: "json",
    parameters: [
      {
        name: "body",
        type: "Body",
        schema: z.object({ password: z.string() }).passthrough(),
      },
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: z.object({ verified: z.boolean() }).passthrough(),
    errors: [
      {
        status: 401,
        description: `Password verification failed`,
        schema: z.void(),
      },
    ],
  },
  {
    method: "post",
    path: "/api/v1/auth/signup",
    alias: "signup",
    requestFormat: "json",
    parameters: [
      {
        name: "body",
        type: "Body",
        schema: SignupRequest,
      },
    ],
    response: AuthResponse,
    errors: [
      {
        status: 400,
        description: `Invalid input or password complexity failure`,
        schema: ErrorResponse,
      },
      {
        status: 409,
        description: `Email already registered`,
        schema: ErrorResponse,
      },
    ],
  },
]);

export const api = new Zodios(endpoints);

export function createApiClient(baseUrl: string, options?: ZodiosOptions) {
  return new Zodios(baseUrl, endpoints, options);
}
