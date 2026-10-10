import { CountChart } from '@/components/ui/CountChart'
import type { NameCount } from '../../types'

export function MemoryTypesCard({ types, total }: { types: NameCount[]; total: number }) {
  return <CountChart data={types.map(t => ({ ...t, name: t.name || 'Unset' }))} label={`Memories by type · ${total.toLocaleString()} total`} />
}
