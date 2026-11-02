import { makeApi, Zodios, type ZodiosOptions } from "@zodios/core";
import { z } from "zod";

const UserProfile = z
  .object({
    id: z.string().uuid(),
    email: z.string().email(),
    role: z.enum([
      "retail_trader",
      "market_maker",
      "market_taker",
      "partner",
      "admin",
    ]),
    kyc_status: z.enum(["unverified", "pending", "verified", "rejected"]),
    status: z.enum(["active", "suspended", "deactivated"]),
    display_name: z.string().nullish(),
    created_at: z.number().int(),
  })
  .passthrough();
const UpdateProfileRequest = z
  .object({
    display_name: z.string().max(50),
    preferences: z.object({}).partial().passthrough(),
  })
  .partial()
  .passthrough();
const ErrorResponse = z
  .object({ code: z.string(), message: z.string() })
  .passthrough();
const SuccessResponse = z.object({ success: z.boolean() }).passthrough();

export const schemas = {
  UserProfile,
  UpdateProfileRequest,
  ErrorResponse,
  SuccessResponse,
};

const endpoints = makeApi([
  {
    method: "get",
    path: "/api/v1/user/:user_id",
    alias: "getUserById",
    requestFormat: "json",
    parameters: [
      {
        name: "user_id",
        type: "Path",
        schema: z.string().uuid(),
      },
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
      {
        name: "x-user-role",
        type: "Header",
        schema: z.enum([
          "retail_trader",
          "market_maker",
          "market_taker",
          "partner",
          "admin",
        ]),
      },
    ],
    response: UserProfile,
    errors: [
      {
        status: 403,
        description: `Forbidden (requires admin or internal role)`,
        schema: z.void(),
      },
      {
        status: 404,
        description: `User not found`,
        schema: z.void(),
      },
    ],
  },
  {
    method: "get",
    path: "/api/v1/user/me",
    alias: "getMyProfile",
    requestFormat: "json",
    parameters: [
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: UserProfile,
    errors: [
      {
        status: 401,
        description: `Unauthorized`,
        schema: z.void(),
      },
      {
        status: 404,
        description: `User not found`,
        schema: z.void(),
      },
    ],
  },
  {
    method: "put",
    path: "/api/v1/user/me",
    alias: "updateMyProfile",
    requestFormat: "json",
    parameters: [
      {
        name: "body",
        type: "Body",
        schema: UpdateProfileRequest,
      },
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: UserProfile,
    errors: [
      {
        status: 400,
        description: `Invalid parameters`,
        schema: ErrorResponse,
      },
      {
        status: 401,
        description: `Unauthorized`,
        schema: z.void(),
      },
    ],
  },
  {
    method: "delete",
    path: "/api/v1/user/me",
    alias: "deleteMyProfile",
    requestFormat: "json",
    parameters: [
      {
        name: "x-user-id",
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
]);

export const api = new Zodios(endpoints);

export function createApiClient(baseUrl: string, options?: ZodiosOptions) {
  return new Zodios(baseUrl, endpoints, options);
}
