import { CountChart } from '@/components/ui/CountChart'
import type { NameCount } from '../../types'

export function TopProjectsCard({ projects }: { projects: NameCount[] }) {
  return <CountChart data={projects} label="Memories by project" color="var(--chart-2)" />
}
