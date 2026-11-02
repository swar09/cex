import { makeApi, Zodios, type ZodiosOptions } from "@zodios/core";
import { z } from "zod";

const CreateOrderRequest = z
  .object({
    client_order_id: z.string().nullish(),
    symbol: z.string(),
    side: z.enum(["buy", "sell"]),
    order_type: z.enum(["limit", "market"]),
    price: z
      .string()
      .regex(/^\d+(\.\d+)?$/)
      .optional(),
    amount: z.string().regex(/^\d+(\.\d+)?$/),
  })
  .passthrough();
const OrderPlacementResponse = z
  .object({
    order_id: z.string().uuid(),
    client_order_id: z.string().nullish(),
    symbol: z.string(),
    side: z.enum(["buy", "sell"]),
    status: z.enum(["received", "queued"]),
    timestamp: z.number().int(),
  })
  .passthrough();
const ErrorResponse = z
  .object({ code: z.string(), message: z.string() })
  .passthrough();
const CancelAllResponse = z
  .object({ cancelled_count: z.number().int(), symbol: z.string().nullable() })
  .passthrough();
const BatchOrderRequest = z
  .object({ orders: z.array(CreateOrderRequest).max(20) })
  .passthrough();
const BatchOrderResponse = z
  .object({
    accepted_count: z.number().int(),
    orders: z.array(OrderPlacementResponse),
  })
  .passthrough();
const OrderCancelResponse = z
  .object({ order_id: z.string().uuid(), status: z.string() })
  .passthrough();
const OrderRecord = z
  .object({
    order_id: z.string().uuid(),
    client_order_id: z.string().nullish(),
    user_id: z.string().uuid(),
    symbol: z.string(),
    side: z.enum(["buy", "sell"]),
    order_type: z.enum(["limit", "market"]),
    price: z
      .string()
      .regex(/^\d+(\.\d+)?$/)
      .nullish(),
    amount: z.string().regex(/^\d+(\.\d+)?$/),
    filled_amount: z.string().regex(/^\d+(\.\d+)?$/),
    status: z.enum([
      "new",
      "partially_filled",
      "filled",
      "cancelled",
      "rejected",
    ]),
    created_at: z.number().int(),
  })
  .passthrough();

export const schemas = {
  CreateOrderRequest,
  OrderPlacementResponse,
  ErrorResponse,
  CancelAllResponse,
  BatchOrderRequest,
  BatchOrderResponse,
  OrderCancelResponse,
  OrderRecord,
};

const endpoints = makeApi([
  {
    method: "post",
    path: "/api/v1/orders",
    alias: "placeOrder",
    requestFormat: "json",
    parameters: [
      {
        name: "body",
        type: "Body",
        schema: CreateOrderRequest,
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
      {
        name: "x-request-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: OrderPlacementResponse,
    errors: [
      {
        status: 400,
        description: `Validation error or invalid price/tick size`,
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
    path: "/api/v1/orders",
    alias: "cancelAllOrders",
    requestFormat: "json",
    parameters: [
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
      {
        name: "symbol",
        type: "Query",
        schema: z.string().optional(),
      },
    ],
    response: CancelAllResponse,
    errors: [
      {
        status: 401,
        description: `Unauthorized`,
        schema: z.void(),
      },
    ],
  },
  {
    method: "delete",
    path: "/api/v1/orders/:order_id",
    alias: "cancelOrder",
    requestFormat: "json",
    parameters: [
      {
        name: "order_id",
        type: "Path",
        schema: z.string().uuid(),
      },
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
      {
        name: "symbol",
        type: "Query",
        schema: z.string(),
      },
    ],
    response: OrderCancelResponse,
    errors: [
      {
        status: 400,
        description: `Missing symbol or invalid order_id`,
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
    path: "/api/v1/orders/batch",
    alias: "placeOrderBatch",
    requestFormat: "json",
    parameters: [
      {
        name: "body",
        type: "Body",
        schema: BatchOrderRequest,
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
      {
        name: "x-request-id",
        type: "Header",
        schema: z.string().uuid(),
      },
    ],
    response: BatchOrderResponse,
    errors: [
      {
        status: 400,
        description: `Exceeded batch ceiling or invalid items`,
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
    path: "/api/v1/orders/history",
    alias: "listOrderHistory",
    requestFormat: "json",
    parameters: [
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
      {
        name: "symbol",
        type: "Query",
        schema: z.string().optional(),
      },
      {
        name: "status",
        type: "Query",
        schema: z
          .enum(["filled", "partially_filled", "cancelled", "rejected"])
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
    response: z.array(OrderRecord),
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
    path: "/api/v1/orders/open",
    alias: "listOpenOrders",
    requestFormat: "json",
    parameters: [
      {
        name: "x-user-id",
        type: "Header",
        schema: z.string().uuid(),
      },
      {
        name: "symbol",
        type: "Query",
        schema: z.string().optional(),
      },
    ],
    response: z.array(OrderRecord),
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
