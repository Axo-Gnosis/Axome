import type { ToolStep } from '../../turns'
import { parseBashDetails } from './bashOutput'
import { record } from './payload'

const RECEIPT_TOOLS: Record<string, true> = {
  spawn: true,
  wait: true,
  kill: true,
  send: true,
  yield: true,
  enter_mode: true
}

export function isReceiptTool(call: ToolStep): boolean {
  return (call.name === 'bash' && parseBashDetails(call.details).job !== null) || RECEIPT_TOOLS[call.name] === true
}

// Mirrors axo's typed tool receipts (`crates/axo/src/coordination.rs`), which the
// coordination tools serialize into `details` verbatim.
const WAIT_REASONS = ['results', 'mail', 'interrupted', 'idle'] as const
type WaitReason = (typeof WAIT_REASONS)[number]
const SEND_KINDS = ['note', 'steer', 'followUp', 'mail'] as const
type SendKind = (typeof SEND_KINDS)[number]

type SpawnReceipt = { kind: 'child'; job: string } | { kind: 'team'; team: string; members: [string, string][] }
type WaitReceipt = { reason: WaitReason; jobs: string[] }
type KillReceipt = { id: string; stopping: boolean }
type SendReceipt = { to: string; kind: SendKind; sessionId: string }

export function spawnReceipt(details: unknown): SpawnReceipt | null {
  const value = record(details)
  if (typeof value.team === 'string') {
    const members = Object.entries(record(value.jobs)).flatMap(([name, job]) =>
      typeof job === 'string' ? [[name, job] as [string, string]] : []
    )
    return { kind: 'team', team: value.team, members }
  }
  return typeof value.job === 'string' ? { kind: 'child', job: value.job } : null
}

export function waitReceipt(details: unknown): WaitReceipt | null {
  const value = record(details)
  if (typeof value.reason !== 'string' || !WAIT_REASONS.some((reason) => reason === value.reason)) return null
  const jobs = Array.isArray(value.jobs) ? value.jobs.filter((job): job is string => typeof job === 'string') : []
  return { reason: value.reason as WaitReason, jobs }
}

export function killReceipt(details: unknown): KillReceipt | null {
  const value = record(details)
  return typeof value.id === 'string' && typeof value.stopping === 'boolean'
    ? { id: value.id, stopping: value.stopping }
    : null
}

export function sendReceipt(details: unknown): SendReceipt | null {
  const value = record(details)
  return typeof value.to === 'string' &&
    typeof value.kind === 'string' &&
    SEND_KINDS.some((kind) => kind === value.kind) &&
    typeof value.sessionId === 'string'
    ? { to: value.to, kind: value.kind as SendKind, sessionId: value.sessionId }
    : null
}
