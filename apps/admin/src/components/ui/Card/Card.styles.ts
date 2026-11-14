import type { CardVariant } from './Card.types';
export const cardBaseStyles = 'glass-1 rounded-xl text-card-foreground';
export const cardVariantStyles:Record<CardVariant,string> = {elevated:'',glass:'',outlined:'',flat:''};
export const cardPaddingStyles = {none:'',sm:'p-4',md:'p-6',lg:'p-6'};
