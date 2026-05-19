/* ========================================
   CARD - STYLES
   ======================================== */

import type { CardVariant } from './Card.types';

export const cardBaseStyles = `
  rounded-2xl transition-apple
`;

export const cardVariantStyles: Record<CardVariant, string> = {
    elevated: `
    bg-surface-primary
    shadow-md
    hover:shadow-lg
  `,
    glass: `
    glass
    border border-border-secondary
    shadow-sm
  `,
    outlined: `
    bg-surface-primary
    border border-border-primary
    hover:border-border-primary
  `,
    flat: `
    bg-surface-secondary
  `,
};

export const cardPaddingStyles = {
    none: '',
    sm: 'p-4',
    md: 'p-6',
    lg: 'p-8',
};