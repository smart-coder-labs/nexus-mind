import type { CardVariant } from './Card.types';
export const cardBaseStyles = 'rounded-xl border bg-card text-card-foreground shadow-sm';
export const cardVariantStyles:Record<CardVariant,string> = {elevated:'',glass:'',outlined:'',flat:''};
export const cardPaddingStyles = {none:'',sm:'p-4',md:'p-6',lg:'p-6'};
