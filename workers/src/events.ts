export type AttemptEventKind = 'started' | 'assistant' | 'tool' | 'usage' | 'result' | 'error'

export interface AttemptEvent {
  kind: AttemptEventKind
  attemptId: string
  text?: string
}

const supportedEventTypes = new Set<AttemptEventKind>([
  'started',
  'assistant',
  'tool',
  'usage',
  'result',
  'error',
])

export function parseClaudeStreamEvent(line: string): AttemptEvent {
  let raw: unknown

  try {
    raw = JSON.parse(line)
  } catch {
    throw new Error('Malformed Claude stream event')
  }

  if (!isRecord(raw) || typeof raw.type !== 'string' || typeof raw.session_id !== 'string') {
    throw new Error('Malformed Claude stream event')
  }

  if (!supportedEventTypes.has(raw.type as AttemptEventKind)) {
    throw new Error(`Unsupported Claude stream event type: ${raw.type}`)
  }

  return {
    kind: raw.type as AttemptEventKind,
    attemptId: raw.session_id,
    text: extractText(raw.message),
  }
}

function extractText(message: unknown): string | undefined {
  if (!isRecord(message) || !Array.isArray(message.content)) {
    return undefined
  }

  const text = message.content.find(
    (content): content is { type: 'text'; text: string } =>
      isRecord(content) && content.type === 'text' && typeof content.text === 'string',
  )

  return text?.text
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}
