import { buttonVariants } from '../shadcn/button';
export const buttonBaseStyles = '';
export const buttonVariantStyles = Object.fromEntries(['primary','secondary','ghost','subtle','outline','destructive'].map(variant=>[variant, buttonVariants({variant:variant==='primary'?'default':variant==='subtle'?'secondary':variant as 'secondary'|'ghost'|'outline'|'destructive'})]));
export const buttonSizeStyles = {sm:'h-8 px-3',md:'h-9 px-4',lg:'h-10 px-6'};
