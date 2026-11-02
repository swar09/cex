import { z } from "zod";
import * as AuthContract from "./generated/auth.js";
import * as UserContract from "./generated/user.js";
import * as KycContract from "./generated/kyc.js";
import * as LedgerContract from "./generated/ledger.js";
import * as OrdersContract from "./generated/orders.js";

export { AuthContract, UserContract, KycContract, LedgerContract, OrdersContract };

// Auth Types & Schemas
export const AuthSchemas = AuthContract.schemas;
export type SignupRequest = z.infer<typeof AuthContract.schemas.SignupRequest>;
export type AuthResponse = z.infer<typeof AuthContract.schemas.AuthResponse>;
export type LoginRequest = z.infer<typeof AuthContract.schemas.LoginRequest>;
export type UpdatePasswordRequest = z.infer<typeof AuthContract.schemas.UpdatePasswordRequest>;
export type VerifyPasswordRequest = z.infer<typeof AuthContract.schemas.VerifyPasswordRequest>;
export type JwksResponse = z.infer<typeof AuthContract.schemas.JwksResponse>;

// User Types & Schemas
export const UserSchemas = UserContract.schemas;
export type UserProfile = z.infer<typeof UserContract.schemas.UserProfile>;
export type UpdateProfileRequest = z.infer<typeof UserContract.schemas.UpdateProfileRequest>;

// KYC Types & Schemas
export const KycSchemas = KycContract.schemas;
export type KycStatusResponse = z.infer<typeof KycContract.schemas.KycStatusResponse>;
export type KycApplicationRequest = z.infer<typeof KycContract.schemas.KycApplicationRequest>;
export type KycSimulateRequest = z.infer<typeof KycContract.schemas.KycSimulateRequest>;

// Ledger Types & Schemas
export const LedgerSchemas = LedgerContract.schemas;
export type AccountBalance = z.infer<typeof LedgerContract.schemas.AccountBalance>;
export type SettlementEstimateRequest = z.infer<typeof LedgerContract.schemas.SettlementEstimateRequest>;
export type SettlementEstimateResponse = z.infer<typeof LedgerContract.schemas.SettlementEstimateResponse>;
export type SettlementExecutionRequest = z.infer<typeof LedgerContract.schemas.SettlementExecutionRequest>;
export type SettlementTicket = z.infer<typeof LedgerContract.schemas.SettlementTicket>;
export type BankingTransaction = z.infer<typeof LedgerContract.schemas.BankingTransaction>;

// Orders Types & Schemas
export const OrdersSchemas = OrdersContract.schemas;
export type CreateOrderRequest = z.infer<typeof OrdersContract.schemas.CreateOrderRequest>;
export type OrderPlacementResponse = z.infer<typeof OrdersContract.schemas.OrderPlacementResponse>;
export type BatchOrderRequest = z.infer<typeof OrdersContract.schemas.BatchOrderRequest>;
export type BatchOrderResponse = z.infer<typeof OrdersContract.schemas.BatchOrderResponse>;
export type OrderCancelResponse = z.infer<typeof OrdersContract.schemas.OrderCancelResponse>;
export type CancelAllResponse = z.infer<typeof OrdersContract.schemas.CancelAllResponse>;
export type OrderRecord = z.infer<typeof OrdersContract.schemas.OrderRecord>;

// Common
export type ErrorResponse = z.infer<typeof OrdersContract.schemas.ErrorResponse>;
export type SuccessResponse = z.infer<typeof AuthContract.schemas.SuccessResponse>;
