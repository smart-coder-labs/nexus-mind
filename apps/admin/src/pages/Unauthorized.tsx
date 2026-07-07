import { Link } from 'react-router-dom'
import { ShieldX } from 'lucide-react'

export default function Unauthorized() {
  return (
    <div className="flex flex-col items-center justify-center h-full min-h-[60vh] gap-4">
      <ShieldX className="w-12 h-12 text-text-tertiary" />
      <h1 className="text-[22px] font-semibold text-text-primary">Access denied</h1>
      <p className="text-[14px] text-text-tertiary">You don't have permission to view this page.</p>
      <Link to="/" className="text-[13px] text-action-blue hover:underline">Go to Dashboard</Link>
    </div>
  )
}
