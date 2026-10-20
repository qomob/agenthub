/**
 * Parser for the v3.1 unified IPC envelope
 * (`docs/03-architecture/API_CONTRACTS.md` §一):
 *
 *   type ApiResult<T> =
 *     | { ok: true; data: T; requestId: string }
 *     | { ok: false; error: { code; message; retryable; details? }; requestId: string }
 *
 * Rust serializes the same shape from `agenthub_core::contract::ApiResult`, so
 * the UI never has to guess whether a command failed: failures are values, not
 * thrown strings. `unwrapApiResult` turns them back into a typed exception with
 * a stable error code for the UI to explain.
 */

import type { ApiErrorPayload, ApiResult } from './types'

/** Structured failure raised for a well-formed `{ok:false}` envelope. */
export class ApiEnvelopeError extends Error {
  readonly code: string
  readonly retryable: boolean
  readonly requestId: string
  readonly details: unknown
  readonly payload: ApiErrorPayload

  constructor(payload: ApiErrorPayload, requestId: string) {
    super(payload.message || payload.code)
    this.name = 'ApiEnvelopeError'
    this.code = payload.code
    this.retryable = payload.retryable
    this.requestId = requestId
    this.details = payload.details
    this.payload = payload
  }
}

/** Malformed envelope (bridge bug or an older backend): fail loudly. */
export class ApiEnvelopeFormatError extends Error {
  readonly raw: unknown

  constructor(message: string, raw: unknown) {
    super(message)
    this.name = 'ApiEnvelopeFormatError'
    this.raw = raw
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function parseErrorPayload(value: unknown): ApiErrorPayload | null {
  if (!isRecord(value)) return null
  if (typeof value.code !== 'string' || value.code.length === 0) return null
  if (typeof value.message !== 'string') return null
  if (typeof value.retryable !== 'boolean') return null
  return {
    code: value.code,
    message: value.message,
    retryable: value.retryable,
    details: value.details,
  }
}

/** Type guard for a syntactically valid envelope. Does not inspect `error`. */
export function isApiResult(value: unknown): value is ApiResult<unknown> {
  if (!isRecord(value)) return false
  if (typeof value.ok !== 'boolean') return false
  if (typeof value.requestId !== 'string' || value.requestId.length === 0) return false
  if (value.ok) return 'data' in value
  return parseErrorPayload(value.error) !== null
}

/**
 * Validate and normalize a raw bridge value into `ApiResult<T>`.
 * Throws `ApiEnvelopeFormatError` when the shape is not the v3.1 contract.
 */
export function parseApiResult<T>(raw: unknown): ApiResult<T> {
  if (!isApiResult(raw)) {
    throw new ApiEnvelopeFormatError(
      '后端返回的数据不符合 ApiResult 契约（缺少 ok/data/error/requestId）。',
      raw,
    )
  }
  if (raw.ok) {
    return { ok: true, data: raw.data as T, requestId: raw.requestId }
  }
  const error = parseErrorPayload(raw.error)
  if (!error) {
    throw new ApiEnvelopeFormatError('后端返回了错误信封，但 error 字段不完整。', raw)
  }
  return { ok: false, error, requestId: raw.requestId }
}

/**
 * Parse an envelope and return `data`, or throw a structured
 * `ApiEnvelopeError` carrying `code` / `retryable` / `requestId`.
 */
export function unwrapApiResult<T>(raw: unknown): T {
  const result = parseApiResult<T>(raw)
  if (result.ok) return result.data
  throw new ApiEnvelopeError(result.error, result.requestId)
}
