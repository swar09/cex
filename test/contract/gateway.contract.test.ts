import { describe, it, expect } from "vitest";
import {
  OrdersSchemas,
  AuthSchemas,
  LedgerSchemas,
  UserSchemas,
  KycSchemas,
} from "../../packages/contracts/src/index.ts";

describe("Gateway API Contracts", () => {
  describe("Orders Service", () => {
    it("accepts order when payload and precision are valid", () => {
      const validPayload = {
        symbol: "BTC_USDT",
        side: "buy",
        order_type: "limit",
        price: "68500.50",
        amount: "0.75",
      };

      const parsed = OrdersSchemas.CreateOrderRequest.safeParse(validPayload);
      expect(parsed.success).toBe(true);

      const invalidPayload = {
        symbol: "BTC_USDT",
        side: "invalid_side",
        order_type: "limit",
        price: "not-a-number",
      };

      const invalidResult = OrdersSchemas.CreateOrderRequest.safeParse(invalidPayload);
      expect(invalidResult.success).toBe(false);
    });

    it("confirms queued state when order placement response is valid", () => {
      const responsePayload = {
        order_id: "7c8234bb-6681-4f9e-a035-7cfa85fba331",
        symbol: "BTC_USDT",
        side: "buy",
        status: "queued",
        timestamp: 1730560000000,
      };

      const parsed = OrdersSchemas.OrderPlacementResponse.safeParse(responsePayload);
      expect(parsed.success).toBe(true);

      const invalidResponse = {
        order_id: "not-a-uuid",
        status: "queued",
      };

      const invalidResult = OrdersSchemas.OrderPlacementResponse.safeParse(invalidResponse);
      expect(invalidResult.success).toBe(false);
    });

    it("accepts batch when all order items conform to schema", () => {
      const batchRequest = {
        orders: [
          {
            symbol: "BTC_USDT",
            side: "buy",
            order_type: "limit",
            price: "68000.00",
            amount: "1.00",
          },
          {
            symbol: "ETH_USDT",
            side: "sell",
            order_type: "market",
            amount: "5.00",
          },
        ],
      };

      const parsedReq = OrdersSchemas.BatchOrderRequest.safeParse(batchRequest);
      expect(parsedReq.success).toBe(true);

      const batchResponse = {
        accepted_count: 2,
        orders: [
          {
            order_id: "7c8234bb-6681-4f9e-a035-7cfa85fba331",
            symbol: "BTC_USDT",
            side: "buy",
            status: "queued",
            timestamp: 1730560000000,
          },
          {
            order_id: "8d9345cc-7792-4fae-b146-8dfb96fca442",
            symbol: "ETH_USDT",
            side: "sell",
            status: "received",
            timestamp: 1730560000001,
          },
        ],
      };

      const parsedRes = OrdersSchemas.BatchOrderResponse.safeParse(batchResponse);
      expect(parsedRes.success).toBe(true);
    });

    it("confirms cancellation when single or mass cancellation is requested", () => {
      const cancelResponse = {
        order_id: "7c8234bb-6681-4f9e-a035-7cfa85fba331",
        status: "cancel_requested",
      };

      const parsedCancel = OrdersSchemas.OrderCancelResponse.safeParse(cancelResponse);
      expect(parsedCancel.success).toBe(true);

      const cancelAllResponse = {
        cancelled_count: 5,
        symbol: "BTC_USDT",
      };

      const parsedCancelAll = OrdersSchemas.CancelAllResponse.safeParse(cancelAllResponse);
      expect(parsedCancelAll.success).toBe(true);
    });

    it("preserves fill state and timestamps when order record is loaded", () => {
      const record = {
        order_id: "7c8234bb-6681-4f9e-a035-7cfa85fba331",
        user_id: "4b684cb3-0098-4c6e-884e-d67b2d28f091",
        symbol: "BTC_USDT",
        side: "buy",
        order_type: "limit",
        price: "68500.00",
        amount: "1.00",
        filled_amount: "0.25",
        status: "partially_filled",
        created_at: 1730560000000,
      };

      const parsed = OrdersSchemas.OrderRecord.safeParse(record);
      expect(parsed.success).toBe(true);
    });
  });

  describe("Auth Service", () => {
    it("accepts credentials when email format is valid", () => {
      const signup = {
        email: "trader@example.com",
        password: "SuperSecretPassword123!",
      };
      expect(AuthSchemas.SignupRequest.safeParse(signup).success).toBe(true);

      const invalidSignup = {
        email: "not-an-email",
        password: "",
      };
      expect(AuthSchemas.SignupRequest.safeParse(invalidSignup).success).toBe(false);

      const login = {
        email: "trader@example.com",
        password: "SuperSecretPassword123!",
      };
      expect(AuthSchemas.LoginRequest.safeParse(login).success).toBe(true);
    });

    it("structures tokens and keys when session is created", () => {
      const authRes = {
        access_token: "header.payload.signature",
        refresh_token: "refresh.payload.signature",
        token_type: "Bearer",
        expires_in: 900,
        user_id: "4b684cb3-0098-4c6e-884e-d67b2d28f091",
      };
      expect(AuthSchemas.AuthResponse.safeParse(authRes).success).toBe(true);

      const jwksRes = {
        keys: [
          {
            kty: "RSA",
            use: "sig",
            kid: "a1b2c3d4e5f678901234567890abcdef",
            alg: "RS256",
            n: "some-modulus-string",
            e: "AQAB",
          },
        ],
      };
      expect(AuthSchemas.JwksResponse.safeParse(jwksRes).success).toBe(true);
    });

    it("verifies credentials when password lifecycle changes occur", () => {
      const updatePwd = {
        current_password: "OldPassword123!",
        new_password: "NewPassword456!",
      };
      expect(AuthSchemas.UpdatePasswordRequest.safeParse(updatePwd).success).toBe(true);

      const verifyPwd = {
        password: "CurrentPassword123!",
      };
      expect(AuthSchemas.VerifyPasswordRequest.safeParse(verifyPwd).success).toBe(true);
    });
  });

  describe("Ledger Service", () => {
    it("returns balance breakdown when account balance is queried", () => {
      const balance = {
        asset: "USDT",
        available: "12500.50",
        reserved: "500.00",
        total: "13000.50",
        updated_at: 1730560000000,
      };
      expect(LedgerSchemas.AccountBalance.safeParse(balance).success).toBe(true);

      const invalidBalance = {
        asset: "",
        available: "-10",
        reserved: "0",
        total: "-10",
        updated_at: 1730560000000,
      };
      expect(LedgerSchemas.AccountBalance.safeParse(invalidBalance).success).toBe(false);
    });

    it("calculates fees and net payout when settlement estimate is requested", () => {
      const estimateReq = {
        asset: "USDT",
        requested_amount: "2500.00",
      };
      expect(LedgerSchemas.SettlementEstimateRequest.safeParse(estimateReq).success).toBe(true);

      const estimateRes = {
        asset: "USDT",
        requested_amount: "2500.00",
        fee_amount: "15.00",
        net_settlement_amount: "2485.00",
        available_balance: "10000.00",
      };
      expect(LedgerSchemas.SettlementEstimateResponse.safeParse(estimateRes).success).toBe(true);
    });

    it("creates settlement ticket when bank payout is submitted", () => {
      const execReq = {
        asset: "USDT",
        amount: "1000.00",
        bank_account_id: "4b684cb3-0098-4c6e-884e-d67b2d28f091",
      };
      expect(LedgerSchemas.SettlementExecutionRequest.safeParse(execReq).success).toBe(true);

      const ticket = {
        ticket_id: "3fa85f64-5717-4562-b3fc-2c963f66afa6",
        user_id: "4b684cb3-0098-4c6e-884e-d67b2d28f091",
        asset: "USDT",
        gross_amount: "1000.00",
        fee_amount: "10.00",
        net_amount: "990.00",
        status: "processing",
        created_at: 1730560000000,
      };
      expect(LedgerSchemas.SettlementTicket.safeParse(ticket).success).toBe(true);
    });

    it("records deposit and withdrawal record when transaction settles", () => {
      const tx = {
        transaction_id: "3fa85f64-5717-4562-b3fc-2c963f66afa6",
        user_id: "4b684cb3-0098-4c6e-884e-d67b2d28f091",
        type: "deposit",
        asset: "USD",
        amount: "5000.00",
        status: "completed",
        created_at: 1730560000000,
      };
      expect(LedgerSchemas.BankingTransaction.safeParse(tx).success).toBe(true);
    });
  });

  describe("User Service", () => {
    it("returns profile information when authenticated user queries me", () => {
      const profile = {
        id: "4b684cb3-0098-4c6e-884e-d67b2d28f091",
        email: "trader@example.com",
        role: "retail_trader",
        kyc_status: "verified",
        status: "active",
        display_name: "Alex Trader",
        created_at: 1730500000000,
      };
      expect(UserSchemas.UserProfile.safeParse(profile).success).toBe(true);

      const updateReq = {
        display_name: "Alexander",
      };
      expect(UserSchemas.UpdateProfileRequest.safeParse(updateReq).success).toBe(true);
    });
  });

  describe("KYC Service", () => {
    it("tracks application status when identification documents are submitted", () => {
      const statusRes = {
        user_id: "4b684cb3-0098-4c6e-884e-d67b2d28f091",
        status: "verified",
        updated_at: 1730560000000,
      };
      expect(KycSchemas.KycStatusResponse.safeParse(statusRes).success).toBe(true);

      const applyReq = {
        document_type: "passport",
        document_number: "N12345678",
        country: "US",
      };
      expect(KycSchemas.KycApplicationRequest.safeParse(applyReq).success).toBe(true);

      const simReq = {
        decision: "pass",
      };
      expect(KycSchemas.KycSimulateRequest.safeParse(simReq).success).toBe(true);
    });
  });

  describe("Error Boundary", () => {
    it("returns standard error shape when request validation fails", () => {
      const errorPayload = {
        code: "INVALID_ARGUMENT",
        message: "The provided price tick size is invalid",
      };
      expect(OrdersSchemas.ErrorResponse.safeParse(errorPayload).success).toBe(true);

      const invalidError = {
        code: 12345,
      };
      expect(OrdersSchemas.ErrorResponse.safeParse(invalidError).success).toBe(false);
    });
  });
});
