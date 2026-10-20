import { describe, expect, it } from 'vitest'

import {
  ApiEnvelopeError,
  ApiEnvelopeFormatError,
  isApiResult,
  parseApiResult,
  unwrapApiResult,
} from '../envelope'

describe('ApiResult envelope', () => {
  it('unwraps a successful envelope and keeps the request id', () => {
    const raw = {
      ok: true,
      data: { operationId: 'op-1', status: 'planned' },
      requestId: 'req-1',
    }
    expect(isApiResult(raw)).toBe(true)
    const parsed = parseApiResult<{ operationId: string; status: string }>(raw)
    expect(parsed.ok).toBe(true)
    expect(parsed.requestId).toBe('req-1')
    expect(unwrapApiResult(raw)).toEqual({ operationId: 'op-1', status: 'planned' })
  })

  it('turns a failure envelope into a structured error with code/retryable/requestId', () => {
    const raw = {
      ok: false,
      error: {
        code: 'PLAN_STALE',
        message: 'plan hash changed; re-preview the operation and approve again',
        retryable: false,
        details: { operationId: 'op-1' },
      },
      requestId: 'req-2',
    }
    expect(() => unwrapApiResult(raw)).toThrow(ApiEnvelopeError)
    try {
      unwrapApiResult(raw)
    } catch (error) {
      const failure = error as ApiEnvelopeError
      expect(failure).toBeInstanceOf(ApiEnvelopeError)
      expect(failure.code).toBe('PLAN_STALE')
      expect(failure.retryable).toBe(false)
      expect(failure.requestId).toBe('req-2')
      expect(failure.details).toEqual({ operationId: 'op-1' })
      expect(failure.message).toContain('re-preview')
    }
  })

  it('rejects malformed bridge payloads instead of guessing', () => {
    expect(isApiResult({ data: {} })).toBe(false)
    expect(() => parseApiResult({ ok: true, requestId: 'req-3' })).toThrow(
      ApiEnvelopeFormatError,
    )
    expect(() =>
      parseApiResult({
        ok: false,
        error: { code: 'INTERNAL', message: 'x' },
        requestId: 'req-4',
      }),
    ).toThrow(ApiEnvelopeFormatError)
    expect(() => unwrapApiResult(null)).toThrow(ApiEnvelopeFormatError)
  })

  it('marks retryable backend failures so the UI can offer a retry honestly', () => {
    const raw = {
      ok: false,
      error: { code: 'UPSTREAM_TIMEOUT', message: 'timeout', retryable: true },
      requestId: 'req-5',
    }
    try {
      unwrapApiResult(raw)
      throw new Error('unreachable')
    } catch (error) {
      expect(error).toBeInstanceOf(ApiEnvelopeError)
      expect((error as ApiEnvelopeError).retryable).toBe(true)
    }
  })
})
