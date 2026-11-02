import { makeApi, Zodios, type ZodiosOptions } from "@zodios/core";
import { z } from "zod";

const AccountBalance = z
  .object({
    asset: z.string(),
    available: z.string().regex(/^\d+(\.\d+)?$/),
    reserved: z.string().regex(/^\d+(\.\d+)?$/),
    total: z.string().regex(/^\d+(\.\d+)?$/),
    updated_at: z.number().int(),
  })
  .passthrough();
const SettlementEstimateRequest = z
  .object({
    asset: z.string(),
    requested_amount: z.string().regex(/^\d+(\.\d+)?$/),
  })
  .passthrough();
const SettlementEstimateResponse = z
  .object({
    asset: z.string(),
    requested_amount: z.string().regex(/^\d+(\.\d+)?$/),
    fee_amount: z.string().regex(/^\d+(\.\d+)?$/),
    net_settlement_amount: z.string().regex(/^\d+(\.\d+)?$/),
    available_balance: z.string().regex(/^\d+(\.\d+)?$/),
  })
  .passthrough();
const ErrorResponse = z
  .object({ code: z.string(), message: z.string() })
  .passthrough();
const SettlementExecutionRequest = z
  .object({
    asset: z.string(),
    amount: z.string().regex(/^\d+(\.\d+)?$/),
    bank_account_id: z.string().uuid(),
  })
  .passthrough();
const SettlementTicket = z
  .object({
    ticket_id: z.string().uuid(),
    user_id: z.string().uuid(),
    asset: z.string(),
    gross_amount: z.string().regex(/^\d+(\.\d+)?$/),
    fee_amount: z.string().regex(/^\d+(\.\d+)?$/),
    net_amount: z.string().regex(/^\d+(\.\d+)?$/),
    status: z.enum(["queued", "processing", "completed", "failed", "rejected"]),
    created_at: z.number().int(),
  })
  .passthrough();
const BankingTransaction = z
  .object({
    transaction_id: z.string().uuid(),
    user_id: z.string().uuid(),
    type: z.enum(["deposit", "withdrawal"]),
    asset: z.string(),
    amount: z.string().regex(/^\d+(\.\d+)?$/),
    status: z.enum(["pending", "completed", "failed", "cancelled"]),
    reference: z.string().optional(),
    created_at: z.number().int(),
  })
  .passthrough();

export const schemas = {
  AccountBalance,
  SettlementEstimateRequest,
  SettlementEstimateResponse,
  ErrorResponse,
  SettlementExecutionRequest,
  SettlementTicket,
  BankingTransaction,
};

const endpoints = makeApi([
  {
    method: "get",
    path: "/api/v1/ledger/balances",
    alias: "getBalances",
    requestFormat: "json",
    parameters: [
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: z.array(AccountBalance),
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
    path: "/api/v1/ledger/balances/:asset",
    alias: "getBalanceByAsset",
    requestFormat: "json",
    parameters: [
      {
        name: "asset",
        type: "Path",
        schema: z.string(),
      },
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: AccountBalance,
    errors: [
      {
        status: 401,
        description: `Unauthorized`,
        schema: z.void(),
      },
      {
        status: 404,
        description: `Asset balance not initialized`,
        schema: z.void(),
      },
    ],
  },
  {
    method: "post",
    path: "/api/v1/ledger/settlement/estimate",
    alias: "estimateSettlement",
    requestFormat: "json",
    parameters: [
      {
        name: "body",
        type: "Body",
        schema: SettlementEstimateRequest,
      },
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: SettlementEstimateResponse,
    errors: [
      {
        status: 400,
        description: `Insufficient funds or invalid calculation`,
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
    path: "/api/v1/ledger/settlement/request",
    alias: "requestSettlement",
    requestFormat: "json",
    parameters: [
      {
        name: "body",
        type: "Body",
        schema: SettlementExecutionRequest,
      },
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: SettlementTicket,
    errors: [
      {
        status: 400,
        description: `Insufficient available balance or invalid bank account`,
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
    method: "get",
    path: "/api/v1/ledger/transactions",
    alias: "listTransactions",
    requestFormat: "json",
    parameters: [
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
      {
        name: "asset",
        type: "Query",
        schema: z.string().optional(),
      },
      {
        name: "type",
        type: "Query",
        schema: z.enum(["deposit", "withdrawal"]).optional(),
      },
      {
        name: "status",
        type: "Query",
        schema: z
          .enum(["pending", "completed", "failed", "cancelled"])
          .optional(),
      },
      {
        name: "limit",
        type: "Query",
        schema: z.number().int().lte(100).optional().default(50),
      },
      {
        name: "offset",
        type: "Query",
        schema: z.number().int().optional().default(0),
      },
    ],
    response: z.array(BankingTransaction),
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
