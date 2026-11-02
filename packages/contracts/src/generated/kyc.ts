import { makeApi, Zodios, type ZodiosOptions } from "@zodios/core";
import { z } from "zod";

const KycStatusResponse = z
  .object({
    user_id: z.string().uuid(),
    status: z.enum(["unverified", "pending", "verified", "rejected"]),
    rejection_reason: z.string().nullish(),
    updated_at: z.number().int(),
  })
  .passthrough();
const KycApplicationRequest = z
  .object({
    document_type: z.enum(["passport", "national_id", "drivers_license"]),
    document_number: z.string(),
    country: z.string(),
  })
  .passthrough();
const ErrorResponse = z
  .object({ code: z.string(), message: z.string() })
  .passthrough();
const KycSimulateRequest = z
  .object({ decision: z.enum(["pass", "fail"]), reason: z.string().optional() })
  .passthrough();

export const schemas = {
  KycStatusResponse,
  KycApplicationRequest,
  ErrorResponse,
  KycSimulateRequest,
};

const endpoints = makeApi([
  {
    method: "post",
    path: "/api/v1/kyc/apply",
    alias: "applyKyc",
    requestFormat: "json",
    parameters: [
      {
        name: "body",
        type: "Body",
        schema: KycApplicationRequest,
      },
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: KycStatusResponse,
    errors: [
      {
        status: 400,
        description: `Invalid document details or already verified`,
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
    method: "post",
    path: "/api/v1/kyc/simulate",
    alias: "simulateKycResult",
    requestFormat: "json",
    parameters: [
      {
        name: "body",
        type: "Body",
        schema: KycSimulateRequest,
      },
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: KycStatusResponse,
    errors: [
      {
        status: 400,
        description: `Invalid simulation action`,
        schema: ErrorResponse,
      },
    ],
  },
  {
    method: "get",
    path: "/api/v1/kyc/status",
    alias: "getKycStatus",
    requestFormat: "json",
    parameters: [
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: KycStatusResponse,
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
