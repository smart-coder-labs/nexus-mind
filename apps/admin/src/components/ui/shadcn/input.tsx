import * as React from 'react';
import { cn } from '@/lib/utils';
import { baseInputStyles } from '../Input/Input.styles';
export function Input({className,...props}:React.ComponentProps<'input'>) {return <input data-slot="input" className={cn(baseInputStyles,'h-9',className)} {...props}/>;}
