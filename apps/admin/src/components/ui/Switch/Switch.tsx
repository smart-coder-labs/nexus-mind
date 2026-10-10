import { useId } from 'react';
import { Switch as PrimitiveSwitch } from '../shadcn/switch';
import { cn } from '@/lib/utils';
export interface SwitchProps {
 checked?:boolean; onCheckedChange?:(checked:boolean)=>void; disabled?:boolean;
 label?:string; description?:string; size?:'sm'|'md'|'lg'; className?:string; 'aria-label'?:string;
}
export function Switch({label, description, size='md', className, ...props}:SwitchProps) {
 const id=useId();
 const control=<PrimitiveSwitch id={id} {...props} aria-label={props['aria-label'] ?? label} aria-describedby={description ? `${id}-description` : undefined} className={cn(size==='lg' && 'scale-110',className)} />;
 if(!label && !description) return control;
 return <div className="flex items-start gap-3"><div className="pt-0.5">{control}</div><div className="min-w-0">
 {label && <label htmlFor={id} className="cursor-pointer text-sm font-medium">{label}</label>}
 {description && <p id={`${id}-description`} className="mt-1 text-sm text-muted-foreground">{description}</p>}
 </div></div>;
}
