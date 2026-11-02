import type { WorkflowDefinition } from './types'
import { Input } from '../../../components/ui/Input'

export function WorkflowEditor({ value, onChange, adapters, repositories }: {value:string;onChange:(value:string)=>void;adapters:string[];repositories:string[]}) {
  let definition:WorkflowDefinition|null=null
  try {
    const parsed=JSON.parse(value) as WorkflowDefinition
    if(parsed && typeof parsed.name==='string' && typeof parsed.workspace_id==='string' && Array.isArray(parsed.steps) && parsed.steps.every(s=>s && typeof s.id==='string' && typeof s.name==='string' && typeof s.adapter==='string' && Array.isArray(s.depends_on) && Array.isArray(s.write_repositories))) definition=parsed
  }catch{/* Keep invalid draft editable in the advanced editor. */}
  const update=(change:(d:WorkflowDefinition)=>void)=>{if(definition){const next=structuredClone(definition);change(next);onChange(JSON.stringify(next,null,2))}}
  return <div className="space-y-4">
    {definition && Array.isArray(definition.steps) && <>
      <label className="block text-xs">Workflow name<Input value={definition.name} onChange={e=>update(d=>{d.name=e.target.value})} required /></label>
      <div className="grid gap-3 sm:grid-cols-2"><label className="block text-xs">Maximum model cost (USD)<Input type="number" min="0.01" max="1000" step="0.01" value={definition.max_cost_usd??20} onChange={e=>update(d=>{d.max_cost_usd=Number(e.target.value)})} /></label><label className="block text-xs">Maximum automatic repair cycles<Input type="number" min="0" max="2" value={definition.max_repairs??2} onChange={e=>update(d=>{d.max_repairs=Number(e.target.value)})} /></label></div>
      {definition.steps.map((step,index)=><fieldset key={step.id} className="space-y-2 rounded-md border border-border-primary p-3"><legend className="px-1 text-sm font-medium">{index+1}. {step.name}</legend><p className="text-xs text-text-tertiary">{step.kind} · after {step.depends_on.join(', ')||'workflow start'}</p>
        <label className="block text-xs">Agent / adapter<select className="mt-1 w-full rounded-md border border-input bg-background p-2 text-sm" value={step.adapter} onChange={e=>update(d=>{d.steps[index].adapter=e.target.value})}>{[...new Set([step.adapter,...adapters])].map(a=><option key={a}>{a}</option>)}</select></label>
        {!adapters.includes(step.adapter) && <p className="text-xs text-text-tertiary">This adapter is not advertised by the selected runner yet. Execution will wait until it is installed.</p>}
        {step.kind==='agent' && <div className="flex flex-wrap gap-3">{repositories.map(path=><label key={path} className="flex items-center gap-2 text-xs"><input type="checkbox" checked={step.write_repositories.includes(path)} onChange={e=>update(d=>{d.steps[index].write_repositories=e.target.checked?[...step.write_repositories,path]:step.write_repositories.filter(p=>p!==path)})} />May modify {path}</label>)}</div>}
        {step.kind==='publish' && <label className="block text-xs">Scoped GitHub connector ID<Input value={String(step.options?.connector_id??'')} onChange={e=>update(d=>{d.steps[index].options={...step.options,connector_id:e.target.value}})} placeholder="Encrypted connector, not a token" required /></label>}
        <label className="block text-xs">Phase instructions<textarea className="mt-1 w-full rounded-md border border-input bg-background p-2 text-sm" value={String(step.options?.instructions??'')} onChange={e=>update(d=>{d.steps[index].options={...step.options,instructions:e.target.value}})} /></label>
        {step.kind==='check' && <label className="block text-xs">Required named checks (comma separated)<Input value={(Array.isArray(step.options?.required_checks)?step.options.required_checks:[]).join(', ')} onChange={e=>update(d=>{d.steps[index].options={...step.options,required_checks:e.target.value.split(',').map(s=>s.trim()).filter(Boolean)}})} /></label>}
      </fieldset>)}
    </>}
    <details><summary className="cursor-pointer text-sm">Advanced definition JSON</summary><label className="block text-xs">Definition JSON<textarea className="mt-2 h-80 w-full rounded-md border border-input bg-background p-3 font-mono text-xs" value={value} onChange={e=>onChange(e.target.value)} /></label></details>
    {!definition && <p role="alert" className="text-sm text-text-secondary">Invalid draft JSON. Fix it in the advanced editor before saving.</p>}
  </div>
}
